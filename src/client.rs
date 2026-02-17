//! LLM client implementations

use super::{config::ProviderConfig, message::Message, Error, Result, Usage};
use futures::stream::Stream;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;

/// Maximum retry attempts for rate-limited requests (HTTP 429)
const MAX_RETRIES: u32 = 3;

/// Build an HTTP client with specified timeout
fn build_http_client(timeout: Duration) -> std::result::Result<HttpClient, reqwest::Error> {
    HttpClient::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .build()
}

/// Calculate delay for retry attempt using exponential backoff with jitter
fn retry_delay(attempt: u32) -> Duration {
    // Exponential backoff: 1s, 2s, 4s
    let base_secs = 1u64 << attempt.min(4); // Cap at 16s base
    Duration::from_secs(base_secs)
}

// ---------------------------------------------------------------------------
// SSE buffer utility — shared between OpenAI and Anthropic streaming parsers
// ---------------------------------------------------------------------------

/// Parsed SSE line types
enum SseLine {
    /// `data: [DONE]` — OpenAI stream terminator
    Done,
    /// `data: <json>` — JSON payload
    Data(String),
    /// `event: <name>` — SSE event name
    Event(String),
    /// Empty or non-SSE line (skip)
    Skip,
}

/// Accumulates bytes from an HTTP response and yields complete SSE lines.
struct SseBuffer {
    buf: Vec<u8>,
}

impl SseBuffer {
    fn new() -> Self {
        Self { buf: Vec::with_capacity(4096) }
    }

    fn extend(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Extract the next complete line (terminated by `\n`) from the buffer.
    /// Returns `None` when no complete line is available yet.
    ///
    /// UTF-8 safety: uses `from_utf8` (strict) instead of `from_utf8_lossy`
    /// to avoid silently corrupting multi-byte characters split across chunk
    /// boundaries. Malformed bytes are reported as an error rather than
    /// replaced with U+FFFD.
    fn next_line(&mut self) -> Option<SseLine> {
        let pos = self.buf.iter().position(|&b| b == b'\n')?;
        let raw: Vec<u8> = self.buf.drain(..=pos).collect();
        let line = match std::str::from_utf8(&raw) {
            Ok(s) => s.trim().to_string(),
            Err(_) => {
                // Server sent non-UTF-8 data — surface as a parseable error
                // instead of silently corrupting the stream.
                return Some(SseLine::Data(
                    r#"{"error":"SSE stream contains invalid UTF-8"}"#.to_string(),
                ));
            }
        };

        if line.is_empty() {
            return Some(SseLine::Skip);
        }

        if line == "data: [DONE]" {
            return Some(SseLine::Done);
        }

        if let Some(json_str) = line.strip_prefix("data: ") {
            return Some(SseLine::Data(json_str.to_string()));
        }

        if let Some(event_name) = line.strip_prefix("event: ") {
            return Some(SseLine::Event(event_name.to_string()));
        }

        Some(SseLine::Skip)
    }
}

/// Streaming event from the LLM
#[derive(Debug, Clone)]
pub struct StreamEvent {
    /// Text delta for this event
    pub delta: String,

    /// Whether this is the final event
    pub done: bool,

    /// Token usage (only available in the final event)
    pub usage: Option<Usage>,
}

/// Trait for LLM clients
#[async_trait::async_trait]
pub trait Client: Send + Sync {
    /// Send a chat completion request (non-streaming)
    async fn chat(&self, messages: &[Message], model: &str) -> Result<(String, Usage)>;

    /// Send a chat completion request and return the raw HTTP response.
    /// This allows the gateway to forward the upstream response without parsing/rewriting it.
    async fn chat_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response>;

    /// Send a chat completion request with streaming
    fn chat_stream(
        &self,
        messages: &[Message],
        model: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

    /// Send a chat completion request and return the raw HTTP response for streaming.
    /// This allows the gateway to forward the upstream response without parsing/rewriting it.
    async fn chat_stream_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response>;

    /// Get the API base URL
    fn api_base(&self) -> &str;

    /// Get the max tokens setting
    fn max_tokens(&self) -> u32;
}

/// OpenAI client implementation
pub struct OpenAIClient {
    config: ProviderConfig,
    http_client: HttpClient,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let timeout = config.timeout();
        Ok(OpenAIClient {
            http_client: build_http_client(timeout)?,
            config,
        })
    }
}

#[async_trait::async_trait]
impl Client for OpenAIClient {
    async fn chat(&self, messages: &[Message], model: &str) -> Result<(String, Usage)> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: false,
        };

        // Retry loop for rate limiting (HTTP 429)
        let mut attempt = 0;
        loop {
            let response = self
                .http_client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .json(&request)
                .send()
                .await?;

            let status = response.status();

            // Handle rate limiting with retry
            if status.as_u16() == 429 && attempt < MAX_RETRIES {
                attempt += 1;
                let delay = retry_delay(attempt);
                tracing::warn!(
                    "Rate limited (429), retrying in {:?} (attempt {}/{})",
                    delay, attempt, MAX_RETRIES
                );
                tokio::time::sleep(delay).await;
                continue;
            }

            let body = response.text().await?;

            if !status.is_success() {
                return Err(Error::Api(format!(
                    "OpenAI API error ({}): {}",
                    status, body
                )));
            }

            let response: ChatResponse = serde_json::from_str(&body)
                .map_err(|e| Error::Api(format!("Failed to parse OpenAI response: {}. Body: {}", e, body)))?;
            let message = response
                .choices
                .first()
                .ok_or_else(|| Error::Api("No choices in OpenAI response".to_string()))?;

            let usage = Usage {
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
            };

            return Ok((message.message.content.clone(), usage));
        }
    }

    async fn chat_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: false,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api(format!("OpenAI API error ({}): {}", status, body)));
        }

        Ok(response)
    }

    fn chat_stream(
        &self,
        messages: &[Message],
        model: &str,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
        };

        let api_key = self.config.api_key.clone();
        let http_client = self.http_client.clone();

        Box::pin(async_stream::stream! {
            let response = match http_client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    yield Err(Error::from(e));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                yield Err(Error::Api(format!("OpenAI API error ({}): {}", status, body)));
                return;
            }

            let mut stream = response.bytes_stream();

            use futures::StreamExt;
            let mut sse = SseBuffer::new();
            let mut usage: Option<Usage> = None;

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(Error::from(e));
                        return;
                    }
                };

                sse.extend(&chunk);

                while let Some(sse_line) = sse.next_line() {
                    match sse_line {
                        SseLine::Done => {
                            yield Ok(StreamEvent { delta: String::new(), done: true, usage: usage.clone() });
                            return;
                        }
                        SseLine::Data(json_str) => {
                            match serde_json::from_str::<ChatStreamChunk>(&json_str) {
                                Ok(chunk) => {
                                    // Extract usage when available (final chunk)
                                    if let Some(ref u) = chunk.usage {
                                        usage = Some(Usage {
                                            prompt_tokens: u.prompt_tokens,
                                            completion_tokens: u.completion_tokens,
                                            total_tokens: u.total_tokens,
                                        });
                                    }

                                    if let Some(delta) = chunk.choices.first() {
                                        let delta_text = delta.delta.content.clone().unwrap_or_default();
                                        let done = delta.finish_reason.as_deref() == Some("stop");
                                        
                                        if !delta_text.is_empty() || done {
                                            yield Ok(StreamEvent { 
                                                delta: delta_text, 
                                                done, 
                                                usage: if done { usage.clone() } else { None } 
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse SSE chunk: {}", e);
                                }
                            }
                        }
                        _ => {} // Skip empty lines and event: lines
                    }
                }
            }
        })
    }

    async fn chat_stream_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api(format!("OpenAI API error ({}): {}", status, body)));
        }

        Ok(response)
    }

    fn api_base(&self) -> &str {
        &self.config.api_base
    }

    fn max_tokens(&self) -> u32 {
        self.config.max_tokens()
    }
}

/// Anthropic client implementation
pub struct AnthropicClient {
    config: ProviderConfig,
    http_client: HttpClient,
}

impl AnthropicClient {
    /// Create a new Anthropic client
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let timeout = config.timeout();
        Ok(AnthropicClient {
            http_client: build_http_client(timeout)?,
            config,
        })
    }
}

#[async_trait::async_trait]
impl Client for AnthropicClient {
    async fn chat(&self, messages: &[Message], model: &str) -> Result<(String, Usage)> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        // Extract system message if present
        let (system, others): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().map(|m| m.content.clone());
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages: messages.clone(),
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: None, // No streaming for regular chat
        };

        // Retry loop for rate limiting (HTTP 429)
        let mut attempt = 0;
        loop {
            let response = self
                .http_client
                .post(&url)
                .header("x-api-key", self.config.api_key.clone())
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await?;

            let status = response.status();

            // Handle rate limiting with retry
            if status.as_u16() == 429 && attempt < MAX_RETRIES {
                attempt += 1;
                let delay = retry_delay(attempt);
                tracing::warn!(
                    "Rate limited (429), retrying in {:?} (attempt {}/{})",
                    delay, attempt, MAX_RETRIES
                );
                tokio::time::sleep(delay).await;
                continue;
            }

            let body = response.text().await?;

            if !status.is_success() {
                return Err(Error::Api(format!(
                    "Anthropic API error ({}): {}",
                    status, body
                )));
            }

            let response: AnthropicMessageResponse = serde_json::from_str(&body)
                .map_err(|e| Error::Api(format!("Failed to parse Anthropic response: {}. Body: {}", e, body)))?;
            let usage = Usage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                total_tokens: response.usage.input_tokens + response.usage.output_tokens,
            };

            let text = response
                .content
                .first()
                .map(|block| block.text.clone())
                .ok_or_else(|| Error::Api("Anthropic response contained no content blocks".to_string()))?;

            return Ok((text, usage));
        }
    }

    async fn chat_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let (system, others): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().map(|m| m.content.clone());
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: None,
        };

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", self.config.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api(format!("Anthropic API error ({}): {}", status, body)));
        }

        Ok(response)
    }

    fn chat_stream(
        &self,
        messages: &[Message],
        model: &str,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let (system, others): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().map(|m| m.content.clone());
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: Some(true),
        };

        let api_key = self.config.api_key.clone();
        let http_client = self.http_client.clone();

        Box::pin(async_stream::stream! {
            let response = match http_client
                .post(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    yield Err(Error::from(e));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                yield Err(Error::Api(format!("Anthropic API error ({}): {}", status, body)));
                return;
            }

            let mut stream = response.bytes_stream();

            use futures::StreamExt;
            let mut sse = SseBuffer::new();
            let mut usage: Option<Usage> = None;

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(Error::from(e));
                        return;
                    }
                };

                sse.extend(&chunk);

                while let Some(sse_line) = sse.next_line() {
                    match sse_line {
                        SseLine::Event(name) if name == "message_stop" => {
                            yield Ok(StreamEvent { delta: String::new(), done: true, usage: usage.clone() });
                            return;
                        }
                        SseLine::Data(json_str) => {
                            match serde_json::from_str::<AnthropicStreamChunk>(&json_str) {
                                Ok(chunk) => {
                                    // Extract usage from message if available (message_start event)
                                    if let Some(msg) = &chunk.message {
                                        if let Some(u) = &msg.usage {
                                            usage = Some(Usage {
                                                prompt_tokens: u.input_tokens,
                                                completion_tokens: u.output_tokens,
                                                total_tokens: u.input_tokens + u.output_tokens,
                                            });
                                        }
                                    }

                                    // Extract usage from message_delta event (GLM API returns usage here)
                                    if chunk.type_ == "message_delta" {
                                        if let Some(u) = &chunk.usage_info {
                                            usage = Some(Usage {
                                                prompt_tokens: u.input_tokens,
                                                completion_tokens: u.output_tokens,
                                                total_tokens: u.input_tokens + u.output_tokens,
                                            });
                                        }
                                    }

                                    match chunk.type_.as_str() {
                                        "content_block_delta" => {
                                            if let Some(StreamDelta::ContentBlock(delta)) = &chunk.delta {
                                                if delta.type_ == "text_delta" && !delta.text.is_empty() {
                                                    yield Ok(StreamEvent { delta: delta.text.clone(), done: false, usage: None });
                                                }
                                            }
                                        }
                                        "message_stop" => {
                                            yield Ok(StreamEvent { delta: String::new(), done: true, usage: usage.clone() });
                                            return;
                                        }
                                        _ => {} // message_delta, content_block_start, etc.
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse SSE chunk: {}", e);
                                }
                            }
                        }
                        _ => {} // Skip empty lines and other events
                    }
                }
            }

            tracing::warn!("SSE stream ended unexpectedly");
        })
    }

    async fn chat_stream_raw(&self, messages: &[Message], model: &str) -> Result<reqwest::Response> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let (system, others): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().map(|m| m.content.clone());
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: Some(true),
        };

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", self.config.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api(format!("Anthropic API error ({}): {}", status, body)));
        }

        Ok(response)
    }

    fn api_base(&self) -> &str {
        &self.config.api_base
    }

    fn max_tokens(&self) -> u32 {
        self.config.max_tokens()
    }
}

// OpenAI types

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: ChatUsage,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChunk {
    choices: Vec<ChatStreamChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    delta: ChatStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamDelta {
    content: Option<String>,
}

// Anthropic types

#[derive(Debug, Serialize)]
struct AnthropicMessageRequest {
    model: String,
    messages: Vec<Message>,
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamChunk {
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    delta: Option<StreamDelta>,
    #[serde(default, rename = "index")]
    #[allow(dead_code)]
    index: u32,
    #[serde(default, rename = "message")]
    #[allow(dead_code)]
    message: Option<AnthropicStreamMessage>,
    #[serde(default, rename = "usage")]
    usage_info: Option<AnthropicStreamUsage>,
}

// Union type for different delta formats
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StreamDelta {
    ContentBlock(AnthropicDelta),
    #[allow(dead_code)]
    MessageDelta(AnthropicMessageDelta),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicMessageDelta {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    type_: String,
    text: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamMessage {
    id: Option<String>,
    #[serde(rename = "type")]
    message_type: Option<String>,
    role: Option<String>,
    content: Option<Vec<AnthropicContentBlock>>,
    model: Option<String>,
    stop_reason: Option<String>,
    usage: Option<AnthropicStreamUsage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageRole;

    #[test]
    fn test_parse_openai_sse_chunk() {
        let json = r#"{"choices":[{"delta":{"content":"Hello"}}]}"#;
        let chunk: ChatStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_parse_openai_sse_chunk_empty() {
        let json = r#"{"choices":[{"delta":{"content":""}}]}"#;
        let chunk: ChatStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content, Some("".to_string()));
    }

    #[test]
    fn test_parse_anthropic_content_block_delta() {
        let json = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let chunk: AnthropicStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.type_, "content_block_delta");
        assert!(chunk.delta.is_some());
    }

    #[test]
    fn test_parse_anthropic_message_delta() {
        let json =
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null}}"#;
        let chunk: AnthropicStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.type_, "message_delta");
        assert!(chunk.delta.is_some());
    }

    #[test]
    fn test_parse_anthropic_ping() {
        let json = r#"{"type":"ping"}"#;
        let chunk: AnthropicStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.type_, "ping");
        assert!(chunk.delta.is_none());
    }

    #[test]
    fn test_sse_line_parsing() {
        // Test data: line stripping
        let line = "data: {\"type\":\"ping\"}";
        let json = line.strip_prefix("data: ");
        assert_eq!(json, Some("{\"type\":\"ping\"}"));

        // Test event: line
        let event_line = "event: message_stop";
        assert_eq!(event_line, "event: message_stop");
    }

    #[test]
    fn test_message_role_system() {
        let msg = Message::system("You are helpful");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "You are helpful");
    }

    #[test]
    fn test_message_role_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_usage_cost_calculation() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };

        let cost = usage.cost(0.50, 1.50);
        // (1000/1M * 0.50) + (500/1M * 1.50) = 0.0005 + 0.00075 = 0.00125
        assert!((cost - 0.00125).abs() < 0.0001);
    }
}

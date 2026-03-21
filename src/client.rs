//! LLM client implementations

use super::{config::ProviderConfig, message::{Message, ToolCall}, Error, Result, Usage};
use futures::stream::Stream;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::time::Duration;

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool parameters (JSON Schema format)
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: String, description: String, parameters: serde_json::Value) -> Self {
        Self { name, description, parameters }
    }

    /// Convert to OpenAI tool definition format
    fn to_openai(&self) -> OpenAIToolDefinition {
        OpenAIToolDefinition {
            tool_type: "function".to_string(),
            function: OpenAIFunctionDefinition {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: self.parameters.clone(),
            },
        }
    }

    /// Convert to Anthropic tool definition format
    fn to_anthropic(&self) -> AnthropicToolDefinition {
        AnthropicToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.parameters.clone(),
        }
    }
}

/// Load tool definitions from a directory (TCL scripts with metadata)
pub fn load_tools_from_dir(tools_dir: Option<&std::path::Path>) -> Result<Vec<ToolDefinition>> {
    let tools_dir = tools_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("tools"));

    if !tools_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tools = Vec::new();

    for entry in std::fs::read_dir(&tools_dir)
        .map_err(|e| Error::Api(format!("Failed to read tools directory: {}", e)))?
    {
        let entry = entry.map_err(|e| Error::Api(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("tcl") {
            continue;
        }

        let _tool_name = path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::Api(format!("Invalid tool filename: {:?}", path)))?;

        // Load tool info using rtcl
        let tool_info = load_tool_info(&path)?;

        // Build JSON Schema for parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (param_name, param_info) in &tool_info.parameters {
            let param_schema = match param_info.param_type.as_str() {
                "string" => json!({"type": "string", "description": param_info.description}),
                "integer" | "int" => json!({"type": "integer", "description": param_info.description}),
                "number" | "float" => json!({"type": "number", "description": param_info.description}),
                "boolean" | "bool" => json!({"type": "boolean", "description": param_info.description}),
                "array" | "list" => json!({"type": "array", "items": {"type": "string"}, "description": param_info.description}),
                _ => json!({"type": "string", "description": param_info.description}),
            };

            properties.insert(param_name.clone(), param_schema);

            if param_info.required {
                required.push(param_name.clone());
            }
        }

        let parameters = if properties.is_empty() {
            json!({"type": "object", "properties": {}, "additionalProperties": false})
        } else {
            json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            })
        };

        tools.push(ToolDefinition::new(
            tool_info.name,
            tool_info.description,
            parameters,
        ));
    }

    tools.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tools)
}

/// Tool metadata extracted from TCL script
#[derive(Debug, Clone)]
struct TclToolInfo {
    name: String,
    description: String,
    parameters: Vec<(String, TclParamInfo)>,
}

#[derive(Debug, Clone)]
struct TclParamInfo {
    param_type: String,
    required: bool,
    description: String,
}

/// Load tool info from a TCL script
fn load_tool_info(script_path: &std::path::Path) -> Result<TclToolInfo> {
    let mut interp = rtcl_core::Interp::new();
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .map_err(|e| Error::Api(format!("Failed to load tool script: {}", e)))?;

    let info_result = interp.eval("info")
        .map_err(|e| Error::Api(format!("Tool script must define 'info' command: {}", e)))?;

    parse_tcl_tool_info(&info_result, script_path)
}

/// Parse tool info from TCL dict value
fn parse_tcl_tool_info(value: &rtcl_core::Value, script_path: &std::path::Path) -> Result<TclToolInfo> {
    let dict = value.as_dict()
        .ok_or_else(|| Error::Api("info command must return a dict".to_string()))?;

    let tool_name = script_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let name = dict.get("name")
        .map(|v| v.as_str().to_string())
        .unwrap_or_else(|| tool_name.clone());

    let description = dict.get("description")
        .map(|v| v.as_str().to_string())
        .ok_or_else(|| Error::Api("Tool must have a description".to_string()))?;

    let mut parameters = Vec::new();
    if let Some(params_value) = dict.get("parameters") {
        if let Some(params_dict) = params_value.as_dict() {
            for (param_name, param_info) in params_dict {
                if let Some(info_dict) = param_info.as_dict() {
                    let param_type = info_dict.get("type")
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_else(|| "string".to_string());
                    let required = info_dict.get("required")
                        .and_then(|v| parse_tcl_bool(v.as_str()))
                        .unwrap_or(false);
                    let description = info_dict.get("description")
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_else(|| String::new());

                    parameters.push((param_name.clone(), TclParamInfo {
                        param_type,
                        required,
                        description,
                    }));
                }
            }
        }
    }

    Ok(TclToolInfo { name, description, parameters })
}

/// Parse a TCL boolean string
fn parse_tcl_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

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

fn normalize_outbound_messages(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .map(|message| match message.role {
            crate::MessageRole::Tool => {
                // If tool message has a tool_call_id, keep it as-is for proper
                // tool result handling by the upstream API
                if message.tool_call_id.is_some() {
                    message.clone()
                } else {
                    // Legacy tool messages without ID: convert to user message
                    if let Some(content) = message.get_content() {
                        Message::user(format!("[Tool Output]\n{}", content))
                    } else {
                        message.clone()
                    }
                }
            }
            _ => message.clone(),
        })
        .collect()
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

    /// Tool calls (when assistant requests tool execution)
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Trait for LLM clients
#[async_trait::async_trait]
pub trait Client: Send + Sync {
    /// Send a chat completion request (non-streaming)
    /// Returns (response_content, tool_calls, usage)
    async fn chat(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<(String, Option<Vec<ToolCall>>, Usage)>;

    /// Send a chat completion request and return the raw HTTP response.
    /// This allows the gateway to forward the upstream response without parsing/rewriting it.
    async fn chat_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response>;

    /// Send a chat completion request with streaming
    fn chat_stream(
        &self,
        messages: &[Message],
        model: &str,
        tools: Option<&[ToolDefinition]>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

    /// Send a chat completion request and return the raw HTTP response for streaming.
    /// This allows the gateway to forward the upstream response without parsing/rewriting it.
    async fn chat_stream_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response>;

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
    async fn chat(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<(String, Option<Vec<ToolCall>>, Usage)> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );

        let normalized_messages = normalize_outbound_messages(messages);
        let openai_messages = messages_to_openai(&normalized_messages);
        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_openai()).collect());
        let request = ChatRequest {
            model: model.to_string(),
            messages: openai_messages,
            stream: false,
            tools: tools_request,
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
            let choice = response
                .choices
                .first()
                .ok_or_else(|| Error::Api("No choices in OpenAI response".to_string()))?;

            let usage = Usage {
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
            };

            // Parse tool calls if present
            let tool_calls = if !choice.message.tool_calls.is_empty() {
                Some(
                    choice.message.tool_calls.iter().map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    }).collect()
                )
            } else {
                None
            };

            return Ok((choice.message.content.clone(), tool_calls, usage));
        }
    }

    async fn chat_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let normalized_messages = normalize_outbound_messages(messages);
        let openai_messages = messages_to_openai(&normalized_messages);
        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_openai()).collect());
        let request = ChatRequest {
            model: model.to_string(),
            messages: openai_messages,
            stream: false,
            tools: tools_request,
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
        tools: Option<&[ToolDefinition]>,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let normalized_messages = normalize_outbound_messages(messages);
        let openai_messages = messages_to_openai(&normalized_messages);
        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_openai()).collect());
        let request = ChatRequest {
            model: model.to_string(),
            messages: openai_messages,
            stream: true,
            tools: tools_request,
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

            // Track accumulated tool calls
            let mut accumulated_tools: std::collections::HashMap<i32, ToolCall> = std::collections::HashMap::new();

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
                            // Yield any accumulated tool calls at the end
                            if !accumulated_tools.is_empty() {
                                let tool_calls: Vec<ToolCall> = accumulated_tools.values().cloned().collect();
                                yield Ok(StreamEvent {
                                    tool_calls: Some(tool_calls),
                                    delta: String::new(),
                                    done: true,
                                    usage: usage.clone(),
                                });
                            } else {
                                yield Ok(StreamEvent {
                                    tool_calls: None,
                                    delta: String::new(),
                                    done: true,
                                    usage: usage.clone(),
                                });
                            }
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
                                        let done = delta.finish_reason.as_deref() == Some("stop") ||
                                                  delta.finish_reason.as_deref() == Some("tool_calls");

                                        // Process tool calls
                                        for tc in &delta.delta.tool_calls {
                                            let entry = accumulated_tools.entry(tc.index).or_insert_with(|| ToolCall {
                                                id: tc.tool_id.clone().unwrap_or_default(),
                                                name: String::new(),
                                                arguments: String::new(),
                                            });

                                            if let Some(ref id) = tc.tool_id {
                                                entry.id = id.clone();
                                            }
                                            if let Some(ref func) = tc.function {
                                                if let Some(ref name) = func.function_name {
                                                    entry.name = name.clone();
                                                }
                                                if let Some(ref args) = func.function_arguments {
                                                    entry.arguments.push_str(args);
                                                }
                                            }
                                        }

                                        // Yield text delta if present
                                        if !delta_text.is_empty() {
                                            yield Ok(StreamEvent {
                                                tool_calls: None,
                                                delta: delta_text,
                                                done: false,
                                                usage: None,
                                            });
                                        }

                                        // Yield tool calls if done
                                        if done && !accumulated_tools.is_empty() {
                                            let tool_calls: Vec<ToolCall> = accumulated_tools.values().cloned().collect();
                                            yield Ok(StreamEvent {
                                                tool_calls: Some(tool_calls),
                                                delta: String::new(),
                                                done: true,
                                                usage: usage.clone(),
                                            });
                                        } else if done {
                                            yield Ok(StreamEvent {
                                                tool_calls: None,
                                                delta: String::new(),
                                                done: true,
                                                usage: usage.clone(),
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

    async fn chat_stream_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let normalized_messages = normalize_outbound_messages(messages);
        let openai_messages = messages_to_openai(&normalized_messages);
        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_openai()).collect());
        let request = ChatRequest {
            model: model.to_string(),
            messages: openai_messages,
            stream: true,
            tools: tools_request,
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
    async fn chat(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<(String, Option<Vec<ToolCall>>, Usage)> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        // Extract system message if present
        let normalized_messages = normalize_outbound_messages(messages);
        let (system, others): (Vec<_>, Vec<_>) = normalized_messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().and_then(|m| m.get_content().map(|s| s.to_string()));
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_anthropic()).collect());
        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages: messages.clone(),
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: None, // No streaming for regular chat
            tools: tools_request,
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

            // Parse content blocks to extract text and tool calls
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for block in &response.content {
                match block {
                    AnthropicContentBlock::Text { text } => {
                        text_parts.push(text.clone());
                    }
                    AnthropicContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: serde_json::to_string(input)
                                .unwrap_or_else(|_| String::new()),
                        });
                    }
                }
            }

            let text = text_parts.join("\n");

            return Ok((text, if tool_calls.is_empty() { None } else { Some(tool_calls) }, usage));
        }
    }

    async fn chat_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let normalized_messages = normalize_outbound_messages(messages);
        let (system, others): (Vec<_>, Vec<_>) = normalized_messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().and_then(|m| m.get_content().map(|s| s.to_string()));
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_anthropic()).collect());
        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: None,
            tools: tools_request,
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
        tools: Option<&[ToolDefinition]>,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let normalized_messages = normalize_outbound_messages(messages);
        let (system, others): (Vec<_>, Vec<_>) = normalized_messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().and_then(|m| m.get_content().map(|s| s.to_string()));
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_anthropic()).collect());
        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: Some(true),
            tools: tools_request,
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

            // Track accumulated tool calls for streaming
            let mut tool_blocks: std::collections::HashMap<u32, ToolCall> = std::collections::HashMap::new();

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
                            // Yield accumulated tool calls if any
                            let tool_calls = if !tool_blocks.is_empty() {
                                let mut calls: Vec<(u32, ToolCall)> = tool_blocks.drain().collect();
                                calls.sort_by_key(|(idx, _)| *idx);
                                Some(calls.into_iter().map(|(_, tc)| tc).collect())
                            } else {
                                None
                            };
                            yield Ok(StreamEvent { tool_calls, delta: String::new(), done: true, usage: usage.clone() });
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
                                        "content_block_start" => {
                                            // Start of a new content block — may be text or tool_use
                                            if let Some(AnthropicStreamContentBlock::ToolUse { id, name, .. }) = &chunk.content_block {
                                                tool_blocks.insert(chunk.index, ToolCall {
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                    arguments: String::new(),
                                                });
                                            }
                                        }
                                        "content_block_delta" => {
                                            if let Some(StreamDelta::ContentBlock(delta)) = &chunk.delta {
                                                match delta.type_.as_str() {
                                                    "text_delta" if !delta.text.is_empty() => {
                                                        yield Ok(StreamEvent { tool_calls: None, delta: delta.text.clone(), done: false, usage: None });
                                                    }
                                                    "input_json_delta" => {
                                                        // Accumulate partial JSON for tool_use arguments
                                                        if let Some(ref partial) = delta.partial_json {
                                                            if let Some(tc) = tool_blocks.get_mut(&chunk.index) {
                                                                tc.arguments.push_str(partial);
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        "message_stop" => {
                                            let tool_calls = if !tool_blocks.is_empty() {
                                                let mut calls: Vec<(u32, ToolCall)> = tool_blocks.drain().collect();
                                                calls.sort_by_key(|(idx, _)| *idx);
                                                Some(calls.into_iter().map(|(_, tc)| tc).collect())
                                            } else {
                                                None
                                            };
                                            yield Ok(StreamEvent { tool_calls, delta: String::new(), done: true, usage: usage.clone() });
                                            return;
                                        }
                                        _ => {} // message_delta, content_block_stop, ping, etc.
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

    async fn chat_stream_raw(&self, messages: &[Message], model: &str, tools: Option<&[ToolDefinition]>) -> Result<reqwest::Response> {
        let url = format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'));

        let normalized_messages = normalize_outbound_messages(messages);
        let (system, others): (Vec<_>, Vec<_>) = normalized_messages
            .iter()
            .partition(|m| m.role == crate::MessageRole::System);

        let system_content = system.first().and_then(|m| m.get_content().map(|s| s.to_string()));
        let messages: Vec<_> = others.into_iter().cloned().collect();

        let tools_request = tools.map(|t| t.iter().map(|tool| tool.to_anthropic()).collect());
        let request = AnthropicMessageRequest {
            model: model.to_string(),
            messages,
            system: system_content,
            max_tokens: self.config.max_tokens(),
            stream: Some(true),
            tools: tools_request,
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

// ---------------------------------------------------------------------------
// OpenAI message format conversion
// ---------------------------------------------------------------------------

/// Convert internal messages to OpenAI API message format.
///
/// OpenAI uses different JSON shapes from Anthropic:
/// - Tool results: `{"role": "tool", "tool_call_id": "...", "content": "..."}`
/// - Assistant tool calls: `{"role": "assistant", "tool_calls": [{...}]}`
fn messages_to_openai(messages: &[Message]) -> Vec<serde_json::Value> {
    messages.iter().map(|msg| {
        // Tool result message → OpenAI tool role
        if msg.role == crate::MessageRole::Tool {
            if let Some(ref tool_call_id) = msg.tool_call_id {
                return json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": msg.get_content().unwrap_or_default()
                });
            }
            // Legacy tool message without ID → convert to user message
            return json!({
                "role": "user",
                "content": format!("[Tool Output]\n{}", msg.get_content().unwrap_or_default())
            });
        }

        // Assistant with tool calls → OpenAI function-call format
        if msg.role == crate::MessageRole::Assistant {
            if let Some(ref calls) = msg.tool_calls {
                let tool_calls: Vec<serde_json::Value> = calls.iter().map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments
                        }
                    })
                }).collect();
                return json!({
                    "role": "assistant",
                    "tool_calls": tool_calls
                });
            }
        }

        // Default: simple role + content
        let role_str = match msg.role {
            crate::MessageRole::System => "system",
            crate::MessageRole::User => "user",
            crate::MessageRole::Assistant => "assistant",
            crate::MessageRole::Tool => "user", // fallback
        };
        json!({
            "role": role_str,
            "content": msg.get_content().unwrap_or_default()
        })
    }).collect()
}

// OpenAI types

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAIToolDefinition>>,
}

#[derive(Debug, Serialize)]
struct OpenAIToolDefinition {
    #[serde(rename = "type")]
    tool_type: String,
    #[serde(rename = "function")]
    function: OpenAIFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
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
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OpenAIToolCall>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
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
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatStreamToolCall>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatStreamToolCall {
    index: i32,
    #[serde(rename = "id")]
    #[serde(default)]
    tool_id: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    tool_type: Option<String>,
    function: Option<ChatStreamFunctionCall>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamFunctionCall {
    #[serde(rename = "name")]
    #[serde(default)]
    function_name: Option<String>,
    #[serde(rename = "arguments")]
    #[serde(default)]
    function_arguments: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDefinition>>,
}

#[derive(Debug, Serialize)]
struct AnthropicToolDefinition {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
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
    /// Content block for content_block_start events
    #[serde(default)]
    content_block: Option<AnthropicStreamContentBlock>,
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
    #[serde(default)]
    text: String,
    /// Partial JSON for input_json_delta events (tool_use arguments)
    #[serde(default)]
    partial_json: Option<String>,
}

/// Content block metadata from content_block_start events
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum AnthropicStreamContentBlock {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        #[allow(dead_code)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        #[allow(dead_code)]
        input: serde_json::Value,
    },
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
        assert_eq!(msg.get_content(), Some("You are helpful"));
    }

    #[test]
    fn test_message_role_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.get_content(), Some("Hello"));
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

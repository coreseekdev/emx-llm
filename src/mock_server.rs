//! Mock HTTP server for testing LLM clients offline
//!
//! This module provides wiremock-based mock servers for OpenAI and Anthropic APIs,
//! allowing tests to run without real API keys.

use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// OpenAI mock server for testing
pub struct OpenAIMockServer {
    server: MockServer,
}

impl OpenAIMockServer {
    /// Create a new OpenAI mock server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the base URL of this mock server
    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Setup a mock response for non-streaming chat completion
    pub async fn mock_chat_completion(&self, content: &str, total_tokens: u32) {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": 1234567890,
                    "model": "glm-4-flash",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": content
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 10,
                        "completion_tokens": total_tokens - 10,
                        "total_tokens": total_tokens
                    }
                })),
            )
            .mount(&self.server)
            .await;
    }

    /// Setup a mock response for streaming chat completion (SSE)
    pub async fn mock_chat_streaming(&self, chunks: Vec<&str>) {
        let mut sse_response = String::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;
            let finish_reason = if is_last {
                Some("stop")
            } else {
                None
            };

            let chunk_json = if let Some(reason) = finish_reason {
                serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion.chunk",
                    "created": 1234567890,
                    "model": "glm-4-flash",
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "content": chunk
                        },
                        "finish_reason": reason
                    }]
                })
            } else {
                serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion.chunk",
                    "created": 1234567890,
                    "model": "glm-4-flash",
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "content": chunk
                        }
                    }]
                })
            };

            sse_response.push_str(&format!("data: {}\n\n", chunk_json));
        }

        sse_response.push_str("data: [DONE]\n\n");

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
            .mount(&self.server)
            .await;
    }
}

/// Anthropic mock server for testing
pub struct AnthropicMockServer {
    server: MockServer,
}

impl AnthropicMockServer {
    /// Create a new Anthropic mock server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the base URL of this mock server
    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Setup a mock response for non-streaming message
    pub async fn mock_message(&self, content: &str, total_tokens: u32) {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "msg-mock",
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "text",
                        "text": content
                    }],
                    "stop_reason": "end_turn",
                    "model": "glm-4-flash",
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": total_tokens - 10
                    }
                })),
            )
            .mount(&self.server)
            .await;
    }

    /// Setup a mock response for streaming message (SSE)
    pub async fn mock_streaming(&self, chunks: Vec<&str>) {
        let mut sse_response = String::new();

        // Send initial event
        sse_response.push_str(&format!(
            "event: message_start\n\
             data: {}\n\n",
            serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": "msg-mock",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "glm-4-flash",
                    "stop_reason": serde_json::Value::Null,
                    "stop_sequence": serde_json::Value::Null,
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": 0
                    }
                }
            })
        ));

        // Send content blocks
        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;

            sse_response.push_str(&format!(
                "event: content_block_start\n\
                 data: {}\n\n",
                serde_json::json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                })
            ));

            sse_response.push_str(&format!(
                "event: content_block_delta\n\
                 data: {}\n\n",
                serde_json::json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "text_delta",
                        "text": chunk
                    }
                })
            ));

            sse_response.push_str(&format!(
                "event: content_block_stop\n\
                 data: {{\"type\": \"content_block_stop\", \"index\": {i}}}\n\n"
            ));

            if is_last {
                sse_response.push_str(&format!(
                    "event: message_delta\n\
                     data: {}\n\n",
                    serde_json::json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": "end_turn",
                            "stop_sequence": serde_json::Value::Null
                        },
                        "usage": {
                            "output_tokens": chunks.len() as u32
                        }
                    })
                ));

                sse_response.push_str("event: message_stop\ndata: {\"type\": \"message_stop\"}\n\n");
            }
        }

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
            .mount(&self.server)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, ProviderConfig, create_client};
    use futures::StreamExt;

    #[tokio::test]
    async fn test_openai_mock_non_streaming() {
        let mock = OpenAIMockServer::start().await;

        mock.mock_chat_completion("Hello, world!", 50).await;

        // Test with real client
        let config = ProviderConfig::openai(
            mock.base_url(),
            "test-key".to_string(),
        );

        let client = create_client(config).unwrap();
        let messages = vec![Message::user("Say hello")];
        let (response, usage) = client.chat(&messages, "glm-4-flash").await.unwrap();

        assert_eq!(response, "Hello, world!");
        assert_eq!(usage.total_tokens, 50);
    }

    #[tokio::test]
    async fn test_openai_mock_streaming() {
        let mock = OpenAIMockServer::start().await;

        mock.mock_chat_streaming(vec!["Hello", ", ", "world", "!"]).await;

        let config = ProviderConfig::openai(
            mock.base_url(),
            "test-key".to_string(),
        );

        let client = create_client(config).unwrap();
        let messages = vec![Message::user("Say hello")];
        let mut stream = client.chat_stream(&messages, "glm-4-flash");

        let mut full_response = String::new();
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            full_response.push_str(&event.delta);
            if event.done {
                break;
            }
        }

        assert_eq!(full_response, "Hello, world!");
    }

    #[tokio::test]
    async fn test_anthropic_mock_non_streaming() {
        let mock = AnthropicMockServer::start().await;

        mock.mock_message("Hello from Anthropic!", 50).await;

        let config = ProviderConfig::anthropic(
            mock.base_url(),
            "test-key".to_string(),
        );

        let client = create_client(config).unwrap();
        let messages = vec![Message::user("Say hello")];
        let (response, usage) = client.chat(&messages, "glm-4-flash").await.unwrap();

        assert_eq!(response, "Hello from Anthropic!");
        assert_eq!(usage.total_tokens, 50);
    }

    #[tokio::test]
    async fn test_anthropic_mock_streaming() {
        let mock = AnthropicMockServer::start().await;

        mock.mock_streaming(vec!["Hello", " from", " Anthropic", "!"]).await;

        let config = ProviderConfig::anthropic(
            mock.base_url(),
            "test-key".to_string(),
        );

        let client = create_client(config).unwrap();
        let messages = vec![Message::user("Say hello")];
        let mut stream = client.chat_stream(&messages, "glm-4-flash");

        let mut full_response = String::new();
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            full_response.push_str(&event.delta);
            if event.done {
                break;
            }
        }

        assert_eq!(full_response, "Hello from Anthropic!");
    }
}

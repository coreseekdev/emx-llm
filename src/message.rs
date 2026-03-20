//! Message types for LLM communication

use serde::{Deserialize, Serialize};

/// Role of a message sender
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System message (sets behavior)
    System,
    /// User message
    User,
    /// Assistant message
    Assistant,
    /// Tool output message
    Tool,
}

impl MessageRole {
    /// Create a new system message
    #[deprecated(note = "Use Message::system() instead — MessageRole factory methods are misleading")]
    pub fn system(content: impl Into<String>) -> Message {
        Message::system(content)
    }

    /// Create a new user message
    #[deprecated(note = "Use Message::user() instead — MessageRole factory methods are misleading")]
    pub fn user(content: impl Into<String>) -> Message {
        Message::user(content)
    }

    /// Create a new assistant message
    #[deprecated(note = "Use Message::assistant() instead — MessageRole factory methods are misleading")]
    pub fn assistant(content: impl Into<String>) -> Message {
        Message::assistant(content)
    }

    /// Create a new tool message
    #[deprecated(note = "Use Message::tool() instead — MessageRole factory methods are misleading")]
    pub fn tool(content: impl Into<String>) -> Message {
        Message::tool(content)
    }
}

/// A single tool call
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID (for OpenAI compatibility)
    pub id: String,
    /// Function name to call
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Content variants for a message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content
    Text(String),
    /// Tool calls (when assistant requests tool execution)
    ToolCalls(Vec<ToolCall>),
}

impl MessageContent {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            MessageContent::ToolCalls(_) => None,
        }
    }

    pub fn is_tool_calls(&self) -> bool {
        matches!(self, MessageContent::ToolCalls(_))
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }
}

/// A chat message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: MessageRole,

    /// Content of the message
    #[serde(flatten)]
    pub content: MessageContent,

    /// Tool call ID (for tool response messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// Tool calls (when assistant requests tool execution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    /// Create a new message with text content
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Message {
            role,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::System,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::User,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Message {
            role: MessageRole::Assistant,
            content: MessageContent::Text(String::new()),
            tool_call_id: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// Create a tool message with result
    pub fn tool_result(tool_call_id: String, content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::Tool,
            content: MessageContent::Text(content.into()),
            tool_call_id: Some(tool_call_id),
            tool_calls: None,
        }
    }

    /// Create a tool message (legacy, for output messages)
    pub fn tool(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::Tool,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Get the text content if present
    pub fn get_content(&self) -> Option<&str> {
        match &self.content {
            MessageContent::Text(s) => Some(s),
            MessageContent::ToolCalls(_) => None,
        }
    }

    /// Check if message has tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.is_some() && self.tool_calls.as_ref().map_or(false, |v| !v.is_empty())
    }

    /// Get the content for serialization (legacy compatibility)
    pub fn content_str(&self) -> String {
        match &self.content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::ToolCalls(calls) => {
                format!("[Tool Calls: {}]", calls.len())
            }
        }
    }
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt
    pub prompt_tokens: u32,

    /// Number of tokens in the completion
    pub completion_tokens: u32,

    /// Total number of tokens
    pub total_tokens: u32,
}

impl Usage {
    /// Calculate cost based on per-million-token pricing
    pub fn cost(&self, prompt_per_million: f64, completion_per_million: f64) -> f64 {
        let prompt_cost = (self.prompt_tokens as f64 / 1_000_000.0) * prompt_per_million;
        let completion_cost =
            (self.completion_tokens as f64 / 1_000_000.0) * completion_per_million;
        prompt_cost + completion_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.get_content(), Some("Hello"));
    }

    #[test]
    fn test_usage_calculation() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };

        let cost = usage.cost(0.50, 1.50);
        assert!((cost - 0.00125).abs() < 0.0001);
    }
}

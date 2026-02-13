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
}

impl MessageRole {
    /// Create a new system message
    #[deprecated(note = "Use Message::system() instead — MessageRole factory methods are misleading")]
    pub fn system(content: impl Into<String>) -> Message {
        Message {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    /// Create a new user message
    #[deprecated(note = "Use Message::user() instead — MessageRole factory methods are misleading")]
    pub fn user(content: impl Into<String>) -> Message {
        Message {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    /// Create a new assistant message
    #[deprecated(note = "Use Message::assistant() instead — MessageRole factory methods are misleading")]
    pub fn assistant(content: impl Into<String>) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }
}

/// A chat message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: MessageRole,

    /// Content of the message
    pub content: String,
}

impl Message {
    /// Create a new message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Message {
            role,
            content: content.into(),
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Message {
            role: MessageRole::Assistant,
            content: content.into(),
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
        assert_eq!(msg.content, "Hello");
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

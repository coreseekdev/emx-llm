# emx-llm

LLM client library for Rust with OpenAI and Anthropic support.

## Overview

`emx-llm` provides a unified, type-safe interface for interacting with Large Language Model (LLM) APIs. It supports streaming responses, async operations, and multi-provider configurations.

## Features

- ✅ **Multi-provider support** - OpenAI, Anthropic
- ✅ **Streaming responses** - Server-Sent Events (SSE) streaming
- ✅ **Type-safe API** - Strongly typed messages and responses
- ✅ **Async/await** - Built on Tokio for async operations
- ✅ **Config management** - YAML configuration files
- ✅ **Mock server** - Built-in testing utilities
- ✅ **Fixture recording** - Record and replay API responses
- ✅ **Cost tracking** - Token usage and cost calculation
- ✅ **MIT License** - Free to use in any project

## Installation

### Library

Add to your `Cargo.toml`:

```toml
[dependencies]
emx-llm = "0.1"
tokio = { version = "1.35", features = ["full"] }
```

Or use via Git:

```toml
[dependencies]
emx-llm = { git = "https://github.com/coreseekdev/emx-llm" }
```

### CLI

```bash
cargo install emx-llm --git https://github.com/coreseekdev/emx-llm
```

## Quick Start

### Library Usage

```rust
use emx_llm::{Client, Provider, Message, Role};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create client
    let client = Client::new(
        Provider::OpenAI,
        "your-api-key-here"
    );

    // Create messages
    let messages = vec![
        Message {
            role: Role::User,
            content: "Hello, how are you?".to_string(),
        }
    ];

    // Send chat request (non-streaming)
    let response = client.chat("gpt-4", &messages).await?;
    println!("Response: {}", response.content);

    Ok(())
}
```

### Streaming

```rust
use emx_llm::{Client, Provider, Message, Role};
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        Provider::OpenAI,
        "your-api-key-here"
    );

    let messages = vec![
        Message {
            role: Role::User,
            content: "Count to 10".to_string(),
        }
    ];

    // Stream response
    let mut stream = client.chat_stream("gpt-4", &messages).await?;

    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?.content);
    }

    Ok(())
}
```

### CLI Usage

```bash
# Set API key
export OPENAI_API_KEY="your-key-here"

# Chat with GPT-4
emx-llm chat "Hello, how are you?"

# Use Anthropic
export ANTHROPIC_AUTH_TOKEN="your-key-here"
emx-llm chat --provider anthropic "Hello"

# Stream response
emx-llm chat --stream "Tell me a story"
```

## Configuration

Create `~/.config/emx-llm/config.yaml`:

```yaml
providers:
  openai:
    api_key: "sk-..."
    base_url: "https://api.openai.com/v1"
    models:
      default: "gpt-4"
      fast: "gpt-3.5-turbo"

  anthropic:
    api_key: "sk-ant-..."
    base_url: "https://api.anthropic.com"
    models:
      default: "claude-3-opus-20240229"
```

Then load config:

```rust
use emx_llm::Config;

let config = Config::load()?;
let client = config.client_for("openai")?;
```

## Providers

### OpenAI

```rust
let client = Client::new(
    Provider::OpenAI,
    "sk-..."
);

let response = client.chat("gpt-4", &messages).await?;
```

### Anthropic

```rust
let client = Client::new(
    Provider::Anthropic,
    "sk-ant-..."
);

let response = client.chat("claude-3-opus-20240229", &messages).await?;
```

## Message Format

```rust
use emx_llm::{Message, Role};

let messages = vec![
    Message {
        role: Role::System,
        content: "You are a helpful assistant.".to_string(),
    },
    Message {
        role: Role::User,
        content: "What is the capital of France?".to_string(),
    },
    Message {
        role: Role::Assistant,
        content: "The capital of France is Paris.".to_string(),
    },
    Message {
        role: Role::User,
        content: "And what about Germany?".to_string(),
    },
];
```

## Testing

### Mock Server

Built-in mock server for testing:

```rust
use emx_llm::mock_server::{MockServer, MockScenario};

#[tokio::test]
async fn test_chat() {
    let mock = MockServer::new().await;

    mock.add_scenario(MockScenario {
        model: "gpt-4".to_string(),
        messages: vec![...],
        response: "Hello!".to_string(),
    });

    let client = mock.client();
    let response = client.chat("gpt-4", &messages).await.unwrap();
    assert_eq!(response.content, "Hello!");
}
```

### Fixture Recording

Record real API responses for replay:

```rust
use emx_llm::FixtureRecorder;

#[tokio::test]
async fn test_with_recording() {
    let recorder = FixtureRecorder::new("tests/fixtures/chat.json");

    // Record mode (set FIXTURE_RECORD=1 env var)
    if recorder.is_recording() {
        let response = real_client.chat("gpt-4", &messages).await?;
        recorder.record(&response)?;
    }

    // Replay mode
    let fixture = recorder.load::<Response>()?;
    assert_eq!(fixture.content, "Hello!");
}
```

## CLI Commands

### Chat

```bash
# Simple chat
emx-llm chat "Hello, AI!"

# Interactive mode
emx-llm chat

# With model
emx-llm chat --model claude-3-opus-20240229 "Hello"

# Stream response
emx-llm chat --stream "Tell me a joke"

# With system message
emx-llm chat --system "You are a poet" "Write a haiku"
```

### Config

```bash
# Show current config
emx-llm config show

# Validate config
emx-llm config validate

# Edit config
emx-llm config edit
```

### Test Mode

```bash
# Test API connection
emx-llm test

# Test with specific provider
emx-llm test --provider anthropic
```

## Cost Tracking

```rust
use emx_llm::{Usage, Cost};

let usage = Usage {
    prompt_tokens: 100,
    completion_tokens: 50,
};

let cost = Cost::calculate(&usage, "gpt-4");
println!("Cost: ${:.4}", cost.total());  // e.g., "Cost: $0.0030"
```

## Advanced Usage

### Custom Base URL

```rust
let client = Client::builder()
    .provider(Provider::OpenAI)
    .api_key("sk-...")
    .base_url("https://custom-proxy.com/v1")
    .build()?;
```

### Retry Logic

```rust
use anyhow::Result;
use tokio::time::{sleep, Duration};

async fn chat_with_retry(
    client: &Client,
    model: &str,
    messages: &[Message],
    max_retries: u32
) -> Result<Response> {
    let mut attempt = 0;

    loop {
        match client.chat(model, messages).await {
            Ok(response) => return Ok(response),
            Err(e) if attempt < max_retries => {
                eprintln!("Attempt {} failed: {}", attempt + 1, e);
                sleep(Duration::from_secs(2u64.pow(attempt))).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Documentation

- [API Documentation](https://docs.rs/emx-llm)
- [Examples](https://github.com/coreseekdev/emx-llm/tree/main/examples)

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Related Projects

- [emx-txtar](https://github.com/coreseekdev/emx-txtar) - Test fixture format
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client library
- [tokio](https://tokio.rs/) - Async runtime

## Acknowledgments

Inspired by:
- [OpenAI Rust SDK](https://github.com/zurawiki/openai-rust)
- [async-openai](https://github.com/zurawiki/async-openai)

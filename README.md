# emx-llm

LLM client library for Rust with OpenAI and Anthropic support.

## Overview

`emx-llm` provides a unified, type-safe interface for interacting with Large Language Model (LLM) APIs. It supports multiple providers (OpenAI, Anthropic), streaming responses, and hierarchical configuration management.

## Features

- ✅ **Multi-provider support** - OpenAI, Anthropic, and compatible APIs
- ✅ **Streaming responses** - Server-Sent Events (SSE) streaming
- ✅ **Type-safe API** - Strongly typed messages and responses
- ✅ **Async/await** - Built on Tokio for async operations
- ✅ **Hierarchical configuration** - Model-specific config inherits from parent sections
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
use emx_llm::{Client, Message, ProviderConfig, create_client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create client
    let config = ProviderConfig {
        provider_type: emx_llm::ProviderType::OpenAI,
        api_base: "https://api.openai.com/v1".to_string(),
        api_key: std::env::var("OPENAI_API_KEY")?,
        model: Some("gpt-4".to_string()),
        max_tokens: Some(4096),
    };

    let client = create_client(config)?;

    // Create messages
    let messages = vec![
        Message::user("Hello, how are you?"),
    ];

    // Send chat request
    let (response, _usage) = client.chat(&messages, "gpt-4").await?;
    println!("Response: {}", response);

    Ok(())
}
```

### Hierarchical Configuration

`emx-llm` supports hierarchical configuration where model-specific settings inherit from parent sections:

```toml
# Default provider type
[llm.provider]
type = "openai"
# Default model to use when no --model argument is provided
default = "anthropic.glm.glm-5"  # Can reference any model in config

# OpenAI-compatible providers
[llm.provider.openai]
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4"
max_tokens = 4096

# Anthropic-compatible providers
[llm.provider.anthropic]
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-3-opus-20240229"
max_tokens = 4096

# Model-specific config (inherits from parent)
[llm.provider.anthropic.sonnet-4.7]
model = "claude-4-sonnet-20250514"

# Third-party Anthropic-compatible provider
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
model = "glm-4.5"

# Model under third-party provider
[llm.provider.anthropic.glm.glm-5]
model = "glm-5"
# api_base and api_key inherited from glm section
```

### CLI Usage

```bash
# Short form (unique in entire config)
emx-llm chat -m glm-5 "query"

# Qualified form with provider
emx-llm chat -m anthropic.glm-5 "query"

# Fully qualified form
emx-llm chat -m anthropic.glm.glm-5 "query"

# Case-insensitive
emx-llm chat -m GLM-5 "query"  # Same as glm-5

# Override API base
emx-llm chat -m glm-5 --api-base https://custom.com/v1 "query"
```

## Configuration

Configuration is loaded from multiple sources in priority order (highest to lowest):

1. **Command-line arguments** - Highest priority
2. **Environment variables** (`EMX_LLM_*` or legacy vars)
3. **Local config file** (`./config.toml`)
4. **Global config file** (`~/.emx/config.toml` or `$EMX_HOME/config.toml`)

### Config File Structure

Create `~/.emx/config.toml`:

```toml
# Default provider type
[llm.provider]
type = "openai"

# OpenAI-compatible providers
[llm.provider.openai]
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4"
max_tokens = 4096

# Anthropic-compatible providers
[llm.provider.anthropic]
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-3-opus-20240229"
max_tokens = 4096

# Third-party Anthropic-compatible provider
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
model = "glm-4.5"

# Model under third-party provider (inherits from parent)
[llm.provider.anthropic.glm.glm-5]
model = "glm-5"
```

### Environment Variables

```bash
# OpenAI
export OPENAI_API_KEY="sk-..."
export OPENAI_API_BASE="https://api.openai.com/v1"

# Anthropic
export ANTHROPIC_AUTH_TOKEN="sk-ant-..."
export ANTHROPIC_BASE_URL="https://api.anthropic.com"

# EMX-LLM prefix (overrides config file)
export EMX_LLM_PROVIDER_TYPE="openai"
export EMX_LLM_PROVIDER_OPENAI_API_KEY="sk-..."
export EMX_LLM_PROVIDER_OPENAI_API_BASE="https://api.openai.com/v1"
```

## Configuration Override Options

The following configuration options can be overridden via CLI arguments or environment variables:

### Provider-Level Overrides

| Option | CLI Arg | Env Var | Description |
|---------|----------|---------|-------------|
| Provider Type | `--provider <type>` | `EMX_LLM_PROVIDER_TYPE` | Provider type (openai/anthropic) |
| API Base | `--api-base <url>` | `EMX_LLM_PROVIDER_OPENAI_API_BASE`<br>`EMX_LLM_PROVIDER_ANTHROPIC_BASE_URL` | API base URL |

### Model-Level Overrides

| Option | CLI Arg | Env Var | Description |
|---------|----------|---------|-------------|
| Model | `--model <name>` | - | Model identifier (supports hierarchical refs like `anthropic.glm.glm-5`) |

### Default Model Configuration

The `[llm.provider]` section supports a `default` option that specifies the default model to use when no `--model` argument is provided:

```toml
[llm.provider]
type = "openai"
default = "anthropic.glm.glm-5"  # Uses this model by default
```

The `default` value is a model reference that will be resolved hierarchically. This means:
- The resolved model may have a different provider type than `llm.provider.type`
- All model-specific settings (api_base, api_key, etc.) are inherited from the model's section

Example: Even when `llm.provider.type = "openai"`, setting `default = "anthropic.glm.glm-5"` will use the Anthropic-compatible configuration for the GLM model.

### Legacy Environment Variables

For backward compatibility, the following legacy environment variables are still supported:

| Provider | API Key Var | Base URL Var |
|----------|---------------|-------------|
| OpenAI | `OPENAI_API_KEY` | `OPENAI_API_BASE` |
| Anthropic | `ANTHROPIC_AUTH_TOKEN` | `ANTHROPIC_BASE_URL` |

## Configuration Key Resolution

When using hierarchical configuration (e.g., `--model glm-5`), configuration keys are resolved with the following priority:

1. **Most specific first** - Check model section (`llm.provider.anthropic.glm.glm-5.*`)
2. **Parent sections** - Search upward through parent sections
3. **Provider defaults** - Fall back to provider-level defaults
4. **Built-in defaults** - Use hardcoded defaults as last resort

Example for `api_base` with model `glm-5`:
```
1. llm.provider.anthropic.glm.glm-5.api_base  → not found
2. llm.provider.anthropic.glm.api_base         → not found
3. llm.provider.anthropic.api_base              → not found
4. (built-in default)                              → https://api.anthropic.com
```

## Library API

### Creating a Client

```rust
use emx_llm::{create_client, ProviderConfig};

// Standard provider config
let config = ProviderConfig {
    provider_type: ProviderType::OpenAI,
    api_base: "https://api.openai.com/v1".to_string(),
    api_key: std::env::var("OPENAI_API_KEY")?,
    model: Some("gpt-4".to_string()),
    max_tokens: Some(4096),
};

let client = create_client(config)?;

// Using hierarchical model reference
let (client, model_id) = emx_llm::create_client_for_model("glm-5")?;
```

### Sending Messages

```rust
use emx_llm::Message;

let messages = vec![
    Message::system("You are a helpful assistant."),
    Message::user("What is Rust?"),
];
```

### Chat Completion (Non-Streaming)

```rust
let (response, usage) = client.chat(&messages, "gpt-4").await?;
println!("Response: {}", response);
println!("Tokens used: {}", usage.total_tokens);
```

### Streaming Chat

```rust
use futures::StreamExt;

let mut stream = client.chat_stream(&messages, "gpt-4");

while let Some(event) = stream.next().await {
    match event {
        Ok(event) => {
            print!("{}", event.delta);
            if event.done {
                println!("\nTokens used: {:?}", event.usage);
            }
        }
        Err(e) => eprintln!("Stream error: {}", e),
    }
}
```

## Providers

### OpenAI

```rust
use emx_llm::{ProviderConfig, create_client};

let config = ProviderConfig {
    provider_type: ProviderType::OpenAI,
    api_base: "https://api.openai.com/v1".to_string(),
    api_key: std::env::var("OPENAI_API_KEY")?,
    model: Some("gpt-4".to_string()),
    max_tokens: Some(4096),
};

let client = create_client(config)?;
```

### Anthropic

```rust
use emx_llm::{ProviderConfig, create_client};

let config = ProviderConfig {
    provider_type: ProviderType::Anthropic,
    api_base: "https://api.anthropic.com".to_string(),
    api_key: std::env::var("ANTHROPIC_AUTH_TOKEN")?,
    model: Some("claude-3-opus-20240229".to_string()),
    max_tokens: Some(4096),
};

let client = create_client(config)?;
```

### Third-Party Providers (Anthropic-Compatible)

```rust
use emx_llm::{ProviderConfig, create_client};

// GLM (Zhipu AI) - Anthropic-compatible API
let config = ProviderConfig {
    provider_type: ProviderType::Anthropic,
    api_base: "https://open.bigmodel.cn/api/paas/v4/".to_string(),
    api_key: std::env::var("GLM_API_KEY")?,
    model: Some("glm-4.5".to_string()),
    max_tokens: Some(4096),
};

let client = create_client(config)?;
```

## CLI Commands

```bash
# Chat with default model
emx-llm chat "Hello, how are you?"

# Specify model
emx-llm chat -m gpt-4 "Hello"

# Interactive mode (no query = enter interactive mode)
emx-llm chat -m gpt-4

# Streaming response
emx-llm chat -m gpt-4 --stream "Tell me a joke"

# With system prompt
emx-llm chat -m gpt-4 --prompt system.txt "query"

# Test configuration
emx-llm test -p openai
```

## Testing

Built-in mock server for testing without real API keys:

```rust
use emx_llm::mock_server::{OpenAIMockServer, AnthropicMockServer};

#[tokio::test]
async fn test_with_mock() {
    let mock = OpenAIMockServer::start().await;
    mock.mock_chat_completion("Hello, world!", 50).await;

    let config = ProviderConfig {
        provider_type: ProviderType::OpenAI,
        api_base: mock.base_url(),
        api_key: "test-key".to_string(),
        model: None,
        max_tokens: None,
    };

    let client = create_client(config).unwrap();
    // ... test with mock
}
```

## Examples

See [examples/](examples/) directory for complete examples.

## Documentation

- [API Documentation](https://docs.rs/emx-llm) - Detailed API documentation
- [USAGE.md](USAGE.md) - CLI usage guide

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

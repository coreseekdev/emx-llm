# emx-llm CLI Usage

## Overview

`emx-llm` is a command-line interface for interacting with LLM providers (OpenAI, Anthropic). It supports both single-query and interactive modes, with streaming and non-streaming output.

## Installation

```bash
cargo install emx-llm
```

## Configuration

Configuration is loaded from multiple sources in priority order (highest to lowest):

1. **Command-line arguments**
2. **Environment variables** (`EMX_LLM_*` or legacy `OPENAI_API_KEY`, `ANTHROPIC_AUTH_TOKEN`)
3. **Local config file** (`./config.toml`)
4. **Global config file** (`~/.emx/config.toml` or `$EMX_HOME/config.toml`)

### Hierarchical Configuration

`emx-llm` supports hierarchical configuration where model-specific settings inherit from parent sections. This allows you to configure multiple models and providers efficiently.

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
# api_base inherited from glm section
# api_key inherited from glm section
```

### Key Resolution Rules

For a model reference like `glm-5`, configuration keys are searched upward:

1. Check `llm.provider.anthropic.glm.glm-5.model` → not found
2. Check `llm.provider.anthropic.glm.model` → not found
3. Check `llm.provider.anthropic.api_base` → found!

Example resolution for `api_base` with model `glm-5`:
```
1. llm.provider.anthropic.glm.glm-5.api_base  → not found
2. llm.provider.anthropic.glm.api_base         → found!
3. Return: https://open.bigmodel.cn/api/paas/v4/
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
| API Base | `--api-base <url>` | `EMX_LLM_PROVIDER_OPENAI_API_BASE`<br>or `EMX_LLM_PROVIDER_ANTHROPIC_BASE_URL` | API base URL |

### Model-Level Overrides

| Option | CLI Arg | Env Var | Description |
|---------|----------|---------|-------------|
| Model | `--model <name>` | - | Model identifier (supports hierarchical refs like `anthropic.glm.glm-5`) |
| Default | `default <ref>` | `EMX_LLM_PROVIDER_DEFAULT` | Default model reference (resolves hierarchically, may differ from provider type) |

### Default Model Configuration

The `[llm.provider]` section supports a `default` option that specifies the default model to use when no `--model` argument is provided:

```toml
[llm.provider]
type = "openai"
default = "anthropic.glm.glm-5"  # Uses this model by default
```

The `default` value is a model reference that will be resolved hierarchically. This allows you to:
- Set a default model that has a different provider type than `llm.provider.type`
- Inherit all model-specific settings (api_base, api_key, etc.) from the model's section

## Commands

### `chat` - Send a chat completion request

```bash
emx-llm chat [OPTIONS] [QUERY]...
```

#### Options

| Option | Short | Long | Description |
|--------|--------|-------|-------------|
| `--provider` | `-p` | Provider type (`openai` or `anthropic`) |
| `--model` | `-m` | Model to use (e.g., `gpt-4`, `claude-3-opus-20240229`) |
| `--api-base` | | API base URL (overrides default) |
| `--stream` | `-s` | Enable streaming output |
| `--prompt` | | System prompt file path |
| `query` | | Query text (if omitted, enters interactive mode) |

#### Model Reference Formats

The `--model` option supports several formats:

```bash
# Short form (unique in entire config)
emx-llm chat -m glm-5 "query"

# Qualified form with provider
emx-llm chat -m anthropic.glm-5 "query"

# Fully qualified form
emx-llm chat -m anthropic.glm.glm-5 "query"
```

**Case-insensitive**: Model references are case-insensitive (`GLM-5` = `glm-5`)

#### Single Query Mode

Send a single query and get the response:

```bash
# Non-streaming
emx-llm chat -m gpt-4 "What is Rust?"

# Streaming
emx-llm chat -m gpt-4 -s "Explain async/await"

# With system prompt
emx-llm chat -m gpt-4 --prompt system.txt "Help me write code"

# Using hierarchical model reference
emx-llm chat -m glm-5 "Hello"

# Override provider explicitly
emx-llm chat -p anthropic -m claude-3-opus-20240229 "Hello"

# Override API base
emx-llm chat -m glm-5 --api-base https://custom.com/v1 "Test"
```

#### Interactive Mode

When no query is provided, `emx-llm` enters interactive mode:

```bash
emx-llm chat -m gpt-4 -s
```

**Interactive commands:**
- Type your message and press Enter to send
- `clear` - Clear conversation history
- `exit` or `quit` - Exit interactive mode
- `Ctrl+D` - Exit (EOF)

**Features:**
- Conversation history is maintained across turns
- Assistant responses are shown in dimmed color
- System prompt from `--prompt` is persisted across conversation

### `test` - Test configuration and API key

```bash
emx-llm test [OPTIONS]
```

#### Options

| Option | Short | Long | Description |
|--------|--------|-------|-------------|
| `--provider` | `-p` | Provider type (default: `openai`) |

#### Examples

```bash
# Test OpenAI configuration
emx-llm test -p openai

# Test Anthropic configuration
emx-llm test -p anthropic
```

**Output on success:**
```
Configuration loaded successfully:
  Provider: OpenAI
  API Base: https://api.openai.com/v1
  API Key: sk-xxxxx***
  Default Model: gpt-4

Configuration is valid!
```

**Output on failure:**
```
Configuration error: API key not found

Make sure to set up your config.toml or environment variables:
...
```

## Examples

### Basic Usage

```bash
# Set API key
export OPENAI_API_KEY="sk-..."

# Simple query
emx-llm chat -m gpt-4 "What is the capital of France?"

# Streaming response
emx-llm chat -m gpt-4 -s "Tell me a joke"
```

### Hierarchical Configuration Example

Create `config.toml`:

```toml
[llm.provider]
type = "anthropic"

[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "your-api-key"
model = "glm-4.5"

[llm.provider.anthropic.glm.glm-5]
model = "glm-5"
```

Use the configured model:

```bash
# Uses glm-5 configuration automatically
emx-llm chat -m glm-5 "Hello from GLM-5!"

# Or with full path
emx-llm chat -m anthropic.glm.glm-5 "Hello from GLM-5!"
```

### With System Prompt

Create `system.txt`:
```
You are a helpful Rust programming assistant.
```

```bash
emx-llm chat -m gpt-4 --prompt system.txt "How do I use async/await?"
```

### Interactive Session

```bash
emx-llm chat -m gpt-4 -s
> Hello! I'm here to help you with Rust programming.
> How do I create a vector?
> [Response...]

> clear
History cleared.

> exit
```

### Anthropic Provider

```bash
export ANTHROPIC_AUTH_TOKEN="sk-ant-..."

emx-llm chat -p anthropic -m claude-3-opus-20240229 "Hello Claude!"
```

### Custom API Endpoint

```bash
emx-llm chat --api-base https://my-proxy.com/v1 -m gpt-4 "Test custom endpoint"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (configuration, API, or user error) |

## Troubleshooting

### "Model not specified" error

Use `--model` flag or configure `model` in config file:

```bash
emx-llm chat -m gpt-4 "query"
```

Or add to `config.toml`:
```toml
[llm.provider.openai]
model = "gpt-4"
```

### "API key not found" error

Set environment variable:

```bash
export OPENAI_API_KEY="sk-..."
```

Or add to `config.toml`:
```toml
[llm.provider.openai]
api_key = "sk-..."
```

### Test your configuration

```bash
emx-llm test -p openai
```

## See Also

- [OpenAI API Docs](https://platform.openai.com/docs)
- [Anthropic API Docs](https://docs.anthropic.com)
- [README.md](./README.md) - Library documentation

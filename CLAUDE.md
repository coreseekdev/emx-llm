# emx-llm - AI Development Guide

## Project Overview

**emx-llm** is a Rust library providing a unified interface for LLM providers (OpenAI, Anthropic). This document provides context for AI assistants working on this codebase.

## Architecture

### Core Components

1. **Client** (`src/client.rs`)
   - Main API for chat completions
   - Supports streaming and non-streaming modes
   - Handles provider-specific differences

2. **Provider** (`src/provider.rs`)
   - Provider enum (OpenAI, Anthropic)
   - Provider-specific configuration
   - Base URL and API key management

3. **Config** (`src/config.rs`)
   - YAML configuration loading
   - Multi-provider support
   - Default model selection

4. **Message** (`src/message.rs`)
   - Message types (User, Assistant, System)
   - Content parsing and formatting
   - Token usage calculation

5. **Mock Server** (`src/mock_server.rs`)
   - HTTP mock server for testing
   - Scenario-based testing
   - SSE streaming simulation

6. **Fixture Recorder** (`src/fixture_recorder.rs`)
   - Record real API responses
   - Replay in tests
   - JSON serialization

## Key Design Decisions

### Unified API

Different providers have different APIs, but `emx-llm` abstracts them:

```rust
// Works for both OpenAI and Anthropic
let response = client.chat(model, &messages).await?;
```

Provider-specific differences handled internally:
- Request format
- Response parsing
- Streaming format
- Error handling

### Streaming via SSE

Both providers use Server-Sent Events (SSE):
- OpenAI: `data: {...}` lines
- Anthropic: `data: {...}` + ping events

Streaming abstraction:
```rust
let stream = client.chat_stream(model, &messages).await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk?.content);
}
```

### Configuration Hierarchy

1. Environment variables (`OPENAI_API_KEY`, `ANTHROPIC_AUTH_TOKEN`)
2. Config file (`~/.config/emx-llm/config.yaml`)
3. Programmatic configuration

### Cost Calculation

Approximate pricing based on token usage:
```rust
let cost = Cost::calculate(&usage, model);
println!("Prompt: ${:.4}", cost.prompt);
println!("Completion: ${:.4}", cost.completion);
println!("Total: ${:.4}", cost.total);
```

## Testing Strategy

### Unit Tests

Located in `src/` modules:
- Message parsing
- Cost calculation
- Provider config
- SSE parsing

### Integration Tests

Using mock server:
```rust
let mock = MockServer::new().await;
mock.add_scenario(scenario);
let client = mock.client();
let response = client.chat(...).await?;
```

### Fixture Testing

Record and replay:
```rust
let recorder = FixtureRecorder::new("fixture.json");
if recorder.is_recording() {
    // Record real API response
    recorder.record(&response)?;
} else {
    // Load from fixture
    let fixture = recorder.load()?;
}
```

Enable recording:
```bash
FIXTURE_RECORD=1 cargo test
```

## Common Tasks

### Adding a New Provider

1. Add variant to `Provider` enum in `src/provider.rs`
2. Implement request formatting
3. Implement response parsing
4. Update `Client::chat()` to handle provider
5. Add mock server support
6. Add tests

Example:
```rust
pub enum Provider {
    OpenAI,
    Anthropic,
    NewProvider,  // Add here
}
```

### Adding a New Model Parameter

1. Extend client builder
2. Pass through to API request
3. Document in README

### Custom Retry Logic

```rust
use tokio::time::{sleep, Duration};

async fn retry<F, Fut>(f: F) -> Result<Response>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<Response>>,
{
    let mut attempts = 0;
    loop {
        match f().await {
            Ok(r) => return Ok(r),
            Err(e) if attempts < 3 => {
                attempts += 1;
                sleep(Duration::from_secs(2u64.pow(attempts))).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Error Handling

Uses `anyhow` for error propagation:
```rust
pub async fn chat(&self, model: &str, messages: &[Message]) -> Result<Response> {
    let request = self.build_request(model, messages)?;
    let http_response = self.http_client.post(&self.url).json(&request).send().await?;
    // ...
}
```

Provider-specific errors:
- Network errors
- API errors (401, 429, 500)
- Parse errors
- Timeout errors

## Performance Considerations

### Streaming vs Non-Streaming

- **Non-streaming**: Wait for full response
- **Streaming**: Process chunks as they arrive
- Use streaming for long responses or real-time applications

### Connection Pooling

`reqwest` handles connection pooling automatically:
```rust
let client = Client::new();  // Reuse for multiple requests
```

### Token Counting

Approximate token counting (not precise):
```rust
// Rough estimate: ~4 chars per token
let estimated_tokens = text.len() / 4;
```

For accurate counts, use provider's tokenization API.

## Code Style

- Use `async fn` for I/O operations
- Return `anyhow::Result<T>` for fallible operations
- Use `?` operator for error propagation
- Document public APIs with examples
- Use `#[cfg(test)]` for test-only code

## Testing with AI

When running tests:
```bash
cargo test
```

Expected output: 21 tests passing

For integration tests with recording:
```bash
FIXTURE_RECORD=1 cargo test
```

## Known Limitations

1. **Token counting** - Approximate, not exact
2. **Rate limiting** - No built-in retry for 429 errors
3. **Timeout** - No configurable timeout
4. **Proxy support** - Limited proxy configuration
5. **Embeddings** - No embeddings API support
6. **Images** - No image input support

## Future Enhancements

Potential improvements:
- [ ] Exact token counting (tiktoken-rs)
- [ ] Automatic retry with exponential backoff
- [ ] Configurable timeouts
- [ ] Embeddings API support
- [ ] Image input (multimodal)
- [ ] Function calling (tools)
- [ ] JSON mode enforcement

## Debugging Tips

### Enable Request Logging

```rust
let client = reqwest::Client::builder()
    .build()?;
```

Set environment variable:
```bash
RUST_LOG=debug=hyper=info cargo run
```

### Mock Server Debugging

```rust
let mock = MockServer::new().await;
mock.add_scenario(scenario);

// Print received requests
mock.inspect_requests();
```

### Fixture Recording

Record real responses:
```bash
FIXTURE_RECORD=1 cargo test test_with_fixture

# Inspect recorded fixture
cat tests/fixtures/chat.json
```

## Security Considerations

- **API Keys**: Never commit API keys
- **Environment Variables**: Use `.env` files locally
- **Key Rotation**: Support key rotation via config
- **TLS**: Always use HTTPS
- **Validation**: Validate API responses

## Best Practices

### API Key Management

```rust
let api_key = std::env::var("OPENAI_API_KEY")
    .expect("OPENAI_API_KEY must be set");
```

### Error Messages

Provide clear error messages:
```rust
Err(anyhow!("Failed to parse response: {}", response_text))
```

### Resource Cleanup

Drop clients when done:
```rust
{
    let client = Client::new(...);
    // Use client
}  // Automatically dropped
```

## See Also

- [OpenAI API Docs](https://platform.openai.com/docs)
- [Anthropic API Docs](https://docs.anthropic.com)
- [reqwest](https://docs.rs/reqwest) - HTTP client
- [tokio](https://tokio.rs/) - Async runtime
- [futures](https://docs.rs/futures) - Stream utilities

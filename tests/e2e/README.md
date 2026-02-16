# emx-gate E2E 测试设计

## 测试策略

使用 `emx-testspec` 进行端到端测试，采用以下模式：

### 核心模式

1. **启动服务器** - 后台运行 emx-gate
2. **发送请求** - 使用 curl 发送 HTTP 请求
3. **验证响应** - 检查状态码、响应内容
4. **清理资源** - 停止服务器

## 测试文件组织

```
emx-llm/tests/
├── e2e/
│   ├── 001-health-check.txtar      # 健康检查测试
│   ├── 002-openai-chat.txtar       # OpenAI 端点测试
│   ├── 003-anthropic-messages.txtar # Anthropic 端点测试
│   ├── 004-model-list.txtar        # 模型列表测试
│   ├── 005-provider-list.txtar     # Provider 列表测试
│   ├── 006-error-handling.txtar    # 错误处理测试
│   └── 007-concurrent-requests.txtar # 并发请求测试
└── mod.rs                           # 测试入口
```

## 测试示例

### 1. 健康检查测试 (`001-health-check.txtar`)

```txtar
# Test health check endpoint

# Start gateway server in background
exec emx-gate &
sleep 2s

# Test health endpoint
exec curl -s http://127.0.0.1:8848/health
stdout '"status":"ok"'

# Test health endpoint with verbose flag
exec curl -s http://127.0.0.1:8848/health
stdout 'timestamp'

# Clean up: stop the server
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 2. OpenAI 聊天端点测试 (`002-openai-chat.txtar`)

```txtar
# Test OpenAI-compatible chat completions endpoint

# Start gateway
exec emx-gate &
sleep 2s

# Test basic chat completion
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Hello"}]}'
stdout '"object":"chat.completion"'
stdout '"model":"openai.gpt-4"'
stdout 'choices'

# Test with system message
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"system","content":"You are a helpful assistant"},{"role":"user","content":"Hi"}]}'
stdout 'assistant'

# Test with temperature parameter
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Test"}],"temperature":0.5}'
stdout 'usage'

# Test error handling: missing model field
! exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
stderr 'Bad Request'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 3. Anthropic 消息端点测试 (`003-anthropic-messages.txtar`)

```txtar
# Test Anthropic-compatible messages endpoint

# Start gateway
exec emx-gate &
sleep 2s

# Test basic message
exec curl -s -X POST http://127.0.0.1:8848/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{"model":"anthropic.claude-3-opus-20240229","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}'
stdout '"type":"message"'
stdout '"role":"assistant"'
stdout 'content'

# Test with multiple messages
exec curl -s -X POST http://127.0.0.1:8848/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{"model":"anthropic.claude-3-opus-20240229","max_tokens":1024,"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"},{"role":"user","content":"How are you?"}]}'
stdout 'stop_reason'

# Test error handling: wrong provider type
! exec curl -s -X POST http://127.0.0.1:8848/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}'
stderr 'Bad Request'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 4. 模型列表测试 (`004-model-list.txtar`)

```txtar
# Test /v1/models endpoint

# Start gateway
exec emx-gate &
sleep 2s

# Test models list
exec curl -s http://127.0.0.1:8848/v1/models
stdout '"object":"list"'
stdout '"data"'
stdout 'openai.gpt-4'
stdout 'anthropic.claude-3-opus-20240229'

# Verify OpenAI model is listed
exec curl -s http://127.0.0.1:8848/v1/models | grep -o '"id":"openai.gpt-4"'
stdout '"id":"openai.gpt-4"'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 5. Provider 列表测试 (`005-provider-list.txtar`)

```txtar
# Test /v1/providers endpoint

# Start gateway
exec emx-gate &
sleep 2s

# Test providers list
exec curl -s http://127.0.0.1:8848/v1/providers
stdout '"object":"list"'
stdout '"data"'
stdout '"id":"openai"'
stdout '"id":"anthropic"'

# Verify provider types
exec curl -s http://127.0.0.1:8848/v1/providers | grep -o '"type":"openai"'
stdout '"type":"openai"'

exec curl -s http://127.0.0.1:8848/v1/providers | grep -o '"type":"anthropic"'
stdout '"type":"anthropic"'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 6. 错误处理测试 (`006-error-handling.txtar`)

```txtar
# Test error handling

# Start gateway
exec emx-gate &
sleep 2s

# Test 404 - unknown model
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"unknown.model","messages":[{"role":"user","content":"Hello"}]}'
stdout 'error'

# Test 400 - malformed JSON
! exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"invalid json'
stderr 'Bad Request'

# Test 405 - wrong method on GET endpoint
! exec curl -s -X POST http://127.0.0.1:8848/health
stderr 'Method Not Allowed'

# Test invalid endpoint
! exec curl -s http://127.0.0.1:8848/invalid
stderr 'Not Found'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 7. 并发请求测试 (`007-concurrent-requests.txtar`)

```txtar
# Test concurrent requests

# Start gateway
exec emx-gate &
sleep 2s

# Send multiple concurrent requests
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Request 1"}]}' &
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Request 2"}]}' &
exec curl -s -X POST http://127.0.0.1:8848/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{"model":"anthropic.claude-3-opus-20240229","max_tokens":100,"messages":[{"role":"user","content":"Request 3"}]}' &
sleep 3s

# All requests should complete successfully
wait

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

## 高级测试技巧

### 使用 Fixtures（测试固件）

```txtar
# Test with request/response fixtures

# Start gateway
exec emx-gate &
sleep 2s

# Prepare request body
cat > request.json << 'EOF'
{
  "model": "openai.gpt-4",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant"},
    {"role": "user", "content": "What is 2+2?"}
  ]
}
EOF

# Send request and save response
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d @request.json > response.txt

# Verify response structure
exec cat response.txt
stdout '"object":"chat.completion"'
stdout '"choices"'

# Verify response has usage information
grep -o '"usage"' response.txt
stdout '"usage"'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
rm request.json response.txt
```

### 配置文件测试

```txtar
# Test with custom configuration

# Create config file
cat > test-config.toml << 'EOF'
[llm]
host = "127.0.0.1"
port = 8849

[llm.provider]
type = "openai"
default = "openai.gpt-4"

[llm.provider.openai]
api_base = "https://api.openai.com/v1"
api_key = "test-key"
model = "gpt-4"
EOF

# Start gateway with custom config
exec emx-gate --config test-config.toml &
sleep 2s

# Test that it's listening on the correct port
exec curl -s http://127.0.0.1:8849/health
stdout '"status":"ok"'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
rm test-config.toml
```

### 使用环境变量

```txtar
# Test with environment variables

# Set environment variables
env EMX_LLM_OPENAI_API_KEY=test-key-123
env EMX_LLM_HOST=127.0.0.1
env EMX_LLM_PORT=8850

# Start gateway
exec emx-gate &
sleep 2s

# Verify it's using the custom port
exec curl -s http://127.0.0.1:8850/health
stdout '"status":"ok"'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

## 测试模块集成

在 `tests/mod.rs` 中集成测试：

```rust
use emx_testspec::run_and_assert;

#[test]
fn test_e2e_health_check() {
    run_and_assert("tests/e2e");
}

#[test]
fn test_e2e_openai_chat() {
    run_and_assert("tests/e2e");
}

#[test]
fn test_e2e_anthropic_messages() {
    run_and_assert("tests/e2e");
}

// 或运行所有测试
#[test]
fn test_e2e_all() {
    let config = emx_testspec::RunConfig {
        dir: "tests/e2e".into(),
        filter: None,
        workdir_root: None,
        preserve_work: false,
        verbose: false,
        extensions: vec![".txtar".into()],
        setup: None,
    };

    let runner = emx_testspec::TestRunner::new(config);
    let result = runner.run_all().expect("Failed to run tests");

    assert!(result.all_passed(), "Some E2E tests failed");
}
```

## 运行测试

```bash
# 运行所有 E2E 测试
cargo test --test e2e

# 运行特定测试
cargo test test_e2e_health_check

# 使用 emx-testspec CLI
emx-testspec tests/e2e/
emx-testspec tests/e2e/ -v              # 详细输出
emx-testspec tests/e2e/ -f "health"    # 过滤测试
emx-testspec tests/e2e/ --keep         # 保留工作目录用于调试
```

## 最佳实践

### 1. 端口冲突处理

```txtar
# Use random port to avoid conflicts

# Find available port and start gateway
env EMX_LLM_PORT=0 exec emx-gate &
sleep 2s

# Get actual port from logs
# (假设 gateway 会打印实际监听的端口)
```

### 2. 测试隔离

每个测试应该：
- 独立启动服务器
- 完成后清理资源
- 不依赖其他测试的状态

### 3. 超时处理

```txtar
# Add timeout for long-running tests

exec emx-gate &
sleep 2s

# Set timeout for curl
exec timeout 10s curl -s http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Hello"}]}'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 4. 调试失败的测试

```bash
# 保留工作目录
emx-testspec tests/e2e/ --keep

# 查看工作目录内容
ls -la /tmp/emx-testspec-*

# 手动运行失败的命令
cd /tmp/emx-testspec-xxx
bash script.txt
```

### 5. 性能测试

```txtar
# Test response time

exec emx-gate &
sleep 2s

# Measure response time
exec time curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Hello"}]}'
stdout 'real.*0ms'  # 应该很快（mock 响应）

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

## 依赖项

确保测试环境有所需工具：

```toml
[dev-dependencies]
emx-testspec = { git = "https://github.com/coreseekdev/emx-testspec" }
emx-txtar = { git = "https://github.com/coreseekdev/emx-txtar" }
```

系统依赖：
- `curl` - HTTP 客户端
- `jq` - JSON 处理（可选，用于复杂验证）
- `timeout` - 超时控制（Unix）
- `taskkill` - 进程管理（Windows）

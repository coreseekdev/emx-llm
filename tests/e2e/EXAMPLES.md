# emx-gate E2E 测试示例

## 现有测试概览

已创建 5 个 E2E 测试文件：

### 1. 健康检查测试

```txtar
# 测试内容
✓ 启动服务器（后台）
✓ 验证 /health 端点返回 200
✓ 检查响应包含 "status":"ok"
✓ 验证时间戳字段存在
✓ 清理服务器进程
```

**运行方式**：
```bash
emx-testspec tests/e2e/001-health-check.txtar
```

### 2. OpenAI 聊天端点测试

```txtar
# 测试内容
✓ POST /v1/chat/completions
✓ 验证响应结构（object, model, choices）
✓ 测试 system message 支持
✓ 验证 usage 信息返回
✓ 响应格式兼容 OpenAI API
```

**示例请求**：
```bash
curl -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "openai.gpt-4",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### 3. Anthropic 消息端点测试

```txtar
# 测试内容
✓ POST /v1/messages
✓ 验证响应结构（type, role, content）
✓ 测试多轮对话
✓ 验证 stop_reason 字段
✓ 响应格式兼容 Anthropic API
```

**示例请求**：
```bash
curl -X POST http://127.0.0.1:8848/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "anthropic.claude-3-opus-20240229",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### 4. 列表端点测试

```txtar
# 测试内容
✓ GET /v1/models
✓ 验证包含 "openai.gpt-4"
✓ 验证包含 "anthropic.claude-3-opus-20240229"
✓ GET /v1/providers
✓ 验证 provider 类型正确
```

### 5. 错误处理测试

```txtar
# 测试内容
✓ 不存在的模型返回 404
✓ 缺少 model 字段返回 400
✓ 无效 JSON 返回错误
✓ 错误的 HTTP 方法返回 405
✓ 不存在的端点返回 404
```

## 运行所有测试

```bash
# 方式 1: 使用 emx-testspec CLI
emx-testspec tests/e2e/

# 方式 2: 使用 cargo test
cargo test --test e2e

# 详细输出
E2E_VERBOSE=1 cargo test --test e2e
```

## 测试执行流程

每个测试遵循相同的生命周期：

```
1. Setup（准备）
   ├─ 启动 emx-gate 服务器
   └─ sleep 2s（等待启动完成）

2. Execute（执行）
   ├─ 发送 HTTP 请求
   ├─ 验证响应内容
   └─ 检查 HTTP 状态码

3. Cleanup（清理）
   └─ 停止服务器进程
```

## 典型测试输出

```
=== E2E Test Summary ===
Total: 5
Passed: 5
Failed: 0

All tests passed! ✓
```

## 失败测试示例

如果测试失败，会看到详细输出：

```
❌ Test: 002-openai-chat.txtar
   Command: exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions ...
   Expected: '"choices"'
   Got: 'error: connection refused'

   💡 Tip: Run with --keep to inspect work directory
   💡 Tip: Check if emx-gate is built: cargo build --bin emx-gate --features gate
```

## 调试技巧

### 1. 保留工作目录

```bash
emx-testspec tests/e2e/ --keep

# 工作目录保留在 /tmp/emx-testspec-xxx
# 可以手动检查日志和输出
```

### 2. 手动运行测试

```bash
# 进入工作目录
cd /tmp/emx-testspec-xxx

# 查看脚本内容
cat script.txt

# 手动执行
bash script.txt
```

### 3. 查看服务器日志

```bash
# 在测试中添加日志输出
exec emx-gate &
sleep 2s

# 保存日志
exec emx-gate > server.log 2>&1 &
sleep 2s

# 执行测试...
exec curl -s http://127.0.0.1:8848/health

# 查看日志
cat server.log

# 清理
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

## 扩展测试

### 添加新的测试文件

```bash
# 1. 创建新的 txtar 文件
touch tests/e2e/006-my-test.txtar

# 2. 编写测试
# 复制现有测试的结构，修改命令和验证

# 3. 运行测试
emx-testspec tests/e2e/006-my-test.txtar
```

### 测试真实 API 调用

```txtar
# Test with real API key

# Set API key
env EMX_LLM_OPENAI_API_KEY=sk-xxx

# Start gateway
exec emx-gate &
sleep 2s

# Send request to real OpenAI API
exec curl -s -X POST http://127.0.0.1:8848/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"openai.gpt-4","messages":[{"role":"user","content":"Hello"}]}'

# Verify real response
stdout '"id":"chatcmpl-"'
stdout '"created":'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

## 相关资源

- [README.md](./README.md) - 完整测试设计文档
- [QUICKSTART.md](./QUICKSTART.md) - 快速开始指南
- [emx-testspec GitHub](https://github.com/coreseekdev/emx-testspec) - 测试框架文档

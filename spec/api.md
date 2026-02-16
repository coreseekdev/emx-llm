# API 接口规范

## 概述

LLM Gateway 提供 HTTP API 接口，客户端通过统一格式访问不同 provider 的模型。

## 设计原则

1. **兼容性优先**：每个 provider 使用原生 API 格式
2. **路径隔离**：不同 provider 使用不同路径前缀
3. **最小转换**：不做 API 格式转换，仅做路由转发

## API 端点设计

### 路径结构

```
/{provider}/{model}/{endpoint}
```

#### OpenAI 兼容接口

```
POST /openai/{model}/chat/completions
POST /openai/{model}/completions
GET  /openai/models
```

示例：
```bash
# OpenAI 官方模型
POST /openai/gpt-4/chat/completions

# 第三方 OpenAI 兼容
POST /openai/llama-3-70b/chat/completions
```

#### Anthropic 兼容接口

```
POST /anthropic/{model}/messages
POST /anthropic/{model}/v1/messages
GET  /anthropic/models
```

示例：
```bash
# Anthropic 官方模型
POST /anthropic/claude-3-opus-20240229/messages

# GLM（Anthropic 兼容）
POST /anthropic/glm-4.5/messages
```

## 请求格式

### OpenAI Chat Completions

**请求**：
```http
POST /openai/gpt-4/chat/completions
Content-Type: application/json
Authorization: Bearer sk-...

{
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "stream": false,
  "temperature": 0.7,
  "max_tokens": 1000
}
```

**响应**：
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "gpt-4",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "Hello! How can I help you today?"
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 9,
    "completion_tokens": 12,
    "total_tokens": 21
  }
}
```

### Anthropic Messages

**请求**：
```http
POST /anthropic/claude-3-opus-20240229/messages
Content-Type: application/json
x-api-key: sk-ant-...
anthropic-version: 2023-06-01

{
  "model": "claude-3-opus-20240229",
  "max_tokens": 1024,
  "messages": [
    {"role": "user", "content": "Hello"}
  ]
}
```

**响应**：
```json
{
  "id": "msg_123",
  "type": "message",
  "role": "assistant",
  "content": [
    {"type": "text", "text": "Hello! How can I help you?"}
  ],
  "model": "claude-3-opus-20240229",
  "stop_reason": "end_turn",
  "usage": {
    "input_tokens": 10,
    "output_tokens": 20
  }
}
```

## 认证方式

### Gateway 认证（可选）

**MVP 阶段**：无认证，直接转发

**未来扩展**：
- API Key 验证
- JWT Token
- OAuth 2.0

### Provider 认证

Gateway 从配置中读取 provider 的 API Key，添加到转发请求中：

| Provider | 认证方式 | Header |
|---------|---------|--------|
| OpenAI | Bearer Token | `Authorization: Bearer sk-...` |
| Anthropic | API Key | `x-api-key: sk-ant-...` |

客户端可以选择提供自己的 API Key（优先级高于配置）：

```bash
# 使用客户端提供的 API Key
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -H "Authorization: Bearer client-provided-key" \
  -H "x-gateway-api-key: sk-custom-..." \
  -d '{"messages": [...]}'
```

## 流式响应

### OpenAI SSE 流

```http
POST /openai/gpt-4/chat/completions
Content-Type: application/json

{
  "stream": true,
  "messages": [...]
}
```

响应：
```
data: {"id":"chatcmpl-123","choices":[{...}]}
data: {"id":"chatcmpl-123","choices":[{...}]}
data: [DONE]
```

### Anthropic SSE 流

```http
POST /anthropic/claude-3-opus-20240229/messages
Content-Type: application/json

{
  "stream": true,
  "messages": [...]
}
```

响应：
```
event: message_start
data: {"type":"message_start",...}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}

event: message_stop
```

## 错误处理

### 错误响应格式

```json
{
  "error": {
    "message": "Invalid model name: unknown-model",
    "type": "invalid_request_error",
    "code": "invalid_model"
  }
}
```

### HTTP 状态码

| 状态码 | 说明 |
|-------|------|
| 200 | 成功 |
| 400 | 请求格式错误 |
| 401 | 认证失败 |
| 404 | 模型或 provider 不存在 |
| 429 | 速率限制（转发 provider 的响应） |
| 500 | Gateway 内部错误 |
| 502 | Provider 错误 |
| 503 | Provider 不可用 |

### 常见错误场景

1. **模型不存在**：404 + 错误信息
2. **Provider 配置缺失**：500 + 配置问题说明
3. **Provider API 错误**：转发 provider 的原始错误
4. **超时**：504 + 超时信息

## 元数据添加

Gateway 可以在响应中添加元数据头：

```http
x-gateway-provider: openai
x-gateway-model: gpt-4
x-gateway-request-id: req-abc123
x-gateway-response-time: 1234ms
```

## 健康检查

```bash
GET /health

{
  "status": "ok",
  "providers": {
    "openai": "ok",
    "anthropic": "ok"
  }
}
```

## 模型列表

```bash
GET /v1/models

{
  "data": [
    {"id": "openai.gpt-4", "object": "model", "owned_by": "openai"},
    {"id": "anthropic.claude-3-opus-20240229", "object": "model", "owned_by": "anthropic"},
    {"id": "anthropic.glm.glm-5", "object": "model", "owned_by": "glm"}
  ]
}
```

## MVP 范围

**第一版本实现**：
- OpenAI Chat Completions API（非流式）
- Anthropic Messages API（非流式）
- 基础错误处理
- Provider 认证（从配置读取）
- 健康检查端点
- 模型列表端点

**暂不实现**：
- 流式响应（可快速迭代添加）
- Gateway 认证
- 请求/响应转换
- 高级错误恢复
- 请求缓存

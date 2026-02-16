# 监控和日志规范

## 概述

监控和日志系统用于跟踪 Gateway 的运行状态、请求流量和错误情况。

## 日志系统

### 日志级别

| 级别 | 用途 | 示例 |
|-----|------|------|
| ERROR | 错误事件 | Provider 连接失败、配置错误 |
| WARN | 警告信息 | 请求超时、降级发生 |
| INFO | 关键信息 | 启动、关闭、配置加载 |
| DEBUG | 调试信息 | 请求详情、响应时间 |

### 日志格式

**结构化日志（JSON）**：
```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "level": "INFO",
  "message": "Request completed",
  "request_id": "req-abc123",
  "provider": "openai",
  "model": "gpt-4",
  "method": "POST",
  "path": "/openai/gpt-4/chat/completions",
  "status": 200,
  "duration_ms": 1234,
  "prompt_tokens": 100,
  "completion_tokens": 200
}
```

**人类可读格式**：
```
2024-01-15 10:30:45 [INFO] req-abc123 POST /openai/gpt-4/chat/completions 200 1234ms
```

### 日志内容

#### 启动日志
```
[INFO] Starting LLM Gateway v0.1.0
[INFO] Loading config from ./config.toml
[INFO] Loaded 3 providers: openai, anthropic, glm
[INFO] Listening on http://127.0.0.1:8080
```

#### 请求日志
```
[INFO] req-abc123 Incoming: POST /openai/gpt-4/chat/completions
[DEBUG] req-abc123 Provider: openai, Model: gpt-4
[DEBUG] req-abc123 Forwarding to https://api.openai.com/v1/chat/completions
[INFO] req-abc123 Completed: 200 1234ms (100+200 tokens)
```

#### 错误日志
```
[ERROR] req-abc124 Provider error: 401 Unauthorized
[WARN] req-abc125 Request timeout after 60s
[ERROR] Configuration error: Missing api_key for openai
```

### 日志输出

**MVP 阶段**：
- 标准输出（stdout）
- 标准错误（stderr）
- 环境变量控制日志级别：`EMX_LLM_LOG_LEVEL=debug`

**未来扩展**：
- 文件轮转
- 日志聚合（ELK、Loki）
- 结构化日志查询

## 监控指标

### 基础指标

#### 请求指标
- **总请求数**：按 provider、模型、状态码分组
- **请求速率**：QPS（每秒请求数）
- **响应时间**：p50、p95、p99 延迟
- **错误率**：4xx、5xx 占比

#### Provider 指标
- **Provider 健康状态**：可用/不可用
- **Provider 响应时间**：各 provider 的平均延迟
- **Provider 错误率**：各 provider 的错误占比

#### Token 使用
- **Prompt Tokens**：输入 token 总数
- **Completion Tokens**：输出 token 总数
- **Total Tokens**：总计 token 数
- **按模型统计**：每个模型的 token 使用量

### 指标导出

**MVP 阶段**：日志输出

**未来扩展**：
- Prometheus 格式（`/metrics` 端点）
- OpenTelemetry 集成
- 自定义 Dashboard

## 请求追踪

### Request ID

每个请求生成唯一 ID：
```
x-gateway-request-id: req-abc123
```

生成规则：
- 格式：`req-{timestamp}-{random}`
- 示例：`req-1705313445-8f3a2b1c`

### 追踪链路

```
Client Request
  ↓
Gateway (req-abc123)
  ↓
Provider API (provider-req-xyz789)
  ↓
Gateway Response
```

日志关联：
```
[INFO] req-abc123 Incoming request
[DEBUG] req-abc123 Provider request ID: provider-req-xyz789
[INFO] req-abc123 Response received
```

## 性能监控

### 响应时间分解

```
Total: 1234ms
├── Gateway Processing: 10ms
├── Network Latency: 50ms
└── Provider Processing: 1174ms
```

### 慢查询日志

记录超过阈值的请求（可配置）：
```toml
[llm.monitoring]
slow_request_threshold_ms = 5000
```

```
[WARN] req-abc126 Slow request: 5678ms (threshold: 5000ms)
```

## 健康检查

### 健康状态

```bash
GET /health

{
  "status": "ok",
  "timestamp": "2024-01-15T10:30:45Z",
  "providers": {
    "openai": {
      "status": "ok",
      "last_check": "2024-01-15T10:30:00Z"
    },
    "anthropic": {
      "status": "ok",
      "last_check": "2024-01-15T10:30:00Z"
    }
  }
}
```

### 就绪检查

```bash
GET /ready

{
  "ready": true,
  "checks": {
    "config_loaded": true,
    "providers_configured": true
  }
}
```

## MVP 范围

**第一版本实现**：
- 结构化日志输出（JSON 可选）
- 四级日志（ERROR/WARN/INFO/DEBUG）
- Request ID 追踪
- 基础性能日志（响应时间、token 使用）
- 健康检查端点
- 环境变量控制日志级别

**暂不实现**：
- Prometheus 指标导出
- 分布式追踪（OpenTelemetry）
- Dashboard
- 告警系统
- 日志聚合
- 慢查询分析（仅日志记录）

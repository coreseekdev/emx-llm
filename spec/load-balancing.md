# 负载均衡和故障转移规范

## 概述

负载均衡和故障转移机制确保 Gateway 的高可用性和容错能力。

## 设计原则

1. **简单优先**：MVP 阶段实现简单的故障转移
2. **可配置**：允许用户自定义策略
3. **透明性**：客户端无感知

## 故障转移（Failover）

### 故障定义

Gateway 认定 Provider 不可用的条件：
- 连接超时
- 连续 N 次请求失败（可配置，默认 3）
- 5xx 错误（服务器内部错误）
- 网络不可达

### 故障转移流程

```
Request: "anthropic.glm.glm-5"
  ↓
Primary Provider: anthropic.glm
  ↓ ✗ Failed (timeout)
  ↓
Fallback: anthropic (default)
  ↓ ✓ Success
  ↓
Return Response
```

### 配置示例

```toml
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
fallback = "anthropic"  # 故障时回退到 anthropic

[llm.provider.anthropic]
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
```

### 故障恢复

- **被动探测**：下次请求时尝试恢复
- **主动探测**（未来）：定期健康检查

## 负载均衡（Load Balancing）

### MVP 阶段

**不实现负载均衡**，每个模型引用对应单一 provider。

### 未来扩展策略

#### 1. 轮询（Round Robin）

```toml
[llm.provider.openai.primary]
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
weight = 1

[llm.provider.openai.backup]
api_base = "https://backup.openai.com/v1"
api_key = "sk-..."
weight = 1
```

请求轮流分发到各个实例。

#### 2. 加权轮询（Weighted Round Robin）

```toml
[llm.provider.openai.primary]
weight = 3  # 75% 流量

[llm.provider.openai.backup]
weight = 1  # 25% 流量
```

#### 3. 最少连接（Least Connections）

选择当前并发请求数最少的 provider。

#### 4. 随机选择（Random）

随机选择一个可用的 provider。

#### 5. 基于性能（Performance-based）

根据响应时间动态调整权重。

## Provider 健康状态

### 状态定义

| 状态 | 说明 | 行为 |
|-----|------|------|
| HEALTHY | 正常运行 | 接收请求 |
| DEGRADED | 性能下降 | 降低优先级 |
| UNHEALTHY | 不可用 | 停止接收请求 |
| DRAINING | 优雅停机 | 完成现有请求，不接收新请求 |

### 健康检查

#### 被动健康检查

基于实际请求结果判断：
- 连续成功 → HEALTHY
- 连续失败 → UNHEALTHY

配置：
```toml
[llm.health_check]
passive = true
failure_threshold = 3      # 连续失败次数
success_threshold = 2      # 连续成功次数
```

#### 主动健康检查（未来）

定期发送探测请求：

```toml
[llm.health_check]
active = true
interval = 30             # 检查间隔（秒）
timeout = 5               # 超时时间（秒）
endpoint = "/models"      # 探测端点
```

### 健康状态 API

```bash
GET /health/providers

{
  "providers": {
    "openai": {
      "status": "HEALTHY",
      "consecutive_failures": 0,
      "last_check": "2024-01-15T10:30:00Z",
      "last_error": null
    },
    "anthropic.glm": {
      "status": "UNHEALTHY",
      "consecutive_failures": 5,
      "last_check": "2024-01-15T10:29:30Z",
      "last_error": "Connection timeout"
    }
  }
}
```

## 断路器模式（Circuit Breaker）

### 状态机

```
CLOSED → OPEN (连续失败)
  ↓
HALF_OPEN (尝试恢复)
  ↓
CLOSED (恢复成功) / OPEN (恢复失败)
```

### 配置

```toml
[llm.circuit_breaker]
enabled = true
failure_threshold = 5      # 触发断路的失败次数
recovery_timeout = 60      # 尝试恢复的等待时间（秒）
half_open_max_calls = 3    # HALF_OPEN 状态的最大试探请求数
```

### 行为

- **CLOSED**：正常请求
- **OPEN**：直接返回错误，不请求 provider
- **HALF_OPEN**：允许少量试探请求

## 超时控制

### 超时层级

```
Gateway Timeout (总超时)
  ↓
  ├─ Connection Timeout (连接超时)
  ├─ Read Timeout (读取超时)
  └─ Provider Timeout (Provider 处理超时)
```

### 配置

```toml
[llm.timeout]
total = 300               # 总超时（秒）
connection = 10           # 连接超时
read = 60                 # 读取超时
```

## 重试机制

### 重试策略

- **指数退避**：每次失败后等待时间翻倍
- **最大重试次数**：默认 3 次
- **仅重试幂等操作**：GET 请求、部分 POST

### 配置

```toml
[llm.retry]
max_attempts = 3
backoff_base = 1          # 初始等待时间（秒）
backoff_multiplier = 2    # 退避倍数
retryable_status = [429, 500, 502, 503, 504]
```

### 重试日志

```
[INFO] req-abc123 Retry 1/3 after 1s (status: 429)
[INFO] req-abc123 Retry 2/3 after 2s (status: 503)
[INFO] req-abc123 Retry 3/3 after 4s (status: 500)
[ERROR] req-abc123 Max retries exceeded
```

## MVP 范围

**第一版本实现**：
- 简单故障转移（fallback 配置）
- 被动健康检查（基于实际请求）
- 基础超时控制
- 健康状态 API

**暂不实现**：
- 负载均衡策略
- 主动健康检查
- 断路器模式
- 重试机制（可快速迭代添加）
- 复杂的状态机

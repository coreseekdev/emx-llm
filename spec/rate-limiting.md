# 限流功能规范

## 概述

限流功能保护 Gateway 和 Provider 不被过度使用，防止恶意攻击和意外超额。

## 设计原则

1. **保护优先**：保护 Gateway 和 Provider 稳定性
2. **可配置**：允许根据需求调整限制
3. **透明性**：客户端清楚知道限制状态

## 限流层级

```
┌─────────────────────────────────────┐
│  Gateway 限流（保护 Gateway）       │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Provider 限流（遵守 Provider 限制）│
└─────────────────────────────────────┘
```

## Gateway 限流

### 限流维度

#### 1. 全局限流

限制整个 Gateway 的请求速率：

```toml
[llm.rate_limit.global]
enabled = true
requests_per_second = 100
```

#### 2. 单 IP 限流

限制每个客户端 IP 的请求速率：

```toml
[llm.rate_limit.ip]
enabled = true
requests_per_minute = 60
burst = 10
```

#### 3. 单 API Key 限流（未来）

限制每个 API Key 的使用：

```toml
[llm.rate_limit.api_key]
key1 = { requests_per_minute = 100 }
key2 = { requests_per_minute = 50 }
```

#### 4. 单模型限流

限制特定模型的使用：

```toml
[llm.rate_limit.model]
"openai.gpt-4" = { requests_per_minute = 10 }
"openai.gpt-3.5-turbo" = { requests_per_minute = 100 }
```

### 限流算法

#### 1. 固定窗口（Fixed Window）

```
时间窗口: 1 分钟
限制: 60 请求
```

问题：边界突变（两倍流量）

#### 2. 滑动窗口（Sliding Window）

```
滑动窗口: 1 分钟
限制: 60 请求
```

更平滑，但需要更多内存。

#### 3. 令牌桶（Token Bucket）

```
容量: 100 令牌
速率: 10 令牌/秒
突发: 允许突发流量
```

适合允许突发的场景。

#### 4. 漏桶（Leaky Bucket）

```
容量: 100 请求
速率: 10 请求/秒
突发: 平滑流量
```

适合强制平滑流量。

### MVP 选择

**固定窗口**（最简单实现）

配置：
```toml
[llm.rate_limit]
algorithm = "fixed_window"
window = 60  # 秒
limit = 100  # 请求数
```

### 限流响应

超过限制时返回：

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json
Retry-After: 60
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1705314000

{
  "error": {
    "message": "Rate limit exceeded. Please retry later.",
    "type": "rate_limit_error",
    "code": "rate_limit_exceeded"
  }
}
```

## Provider 限流

### Provider 限制

遵守各个 Provider 的限流策略：

| Provider | 限制类型 | 默认值 |
|---------|---------|--------|
| OpenAI | RPM/TPM | 按套餐 |
| Anthropic | TPM | 按套餐 |

### 自动限流（未来）

Gateway 自动识别 Provider 限流并遵守：

```toml
[llm.provider.openai]
api_key = "sk-..."
# Gateway 自动识别套餐类型和限制
```

### 手动配置

```toml
[llm.provider.openai]
api_key = "sk-..."
rate_limit = {
    requests_per_minute = 3500,    # RPM
    tokens_per_minute = 90000      # TPM
}
```

### 429 处理

收到 Provider 429 响应时：

1. 解析 `Retry-After` 头
2. 延迟后重试
3. 或返回 429 给客户端

```
[WARN] req-abc123 Provider rate limited (429), retry after 5s
[INFO] req-abc123 Retry 1/3 after 5s
```

## 配额管理（未来）

### Token 配额

```toml
[llm.quota]
daily_tokens = 1000000
monthly_cost = 100.0  # USD
```

### 用量追踪

```bash
GET /quota

{
  "daily_tokens": {
    "limit": 1000000,
    "used": 500000,
    "remaining": 500000
  },
  "monthly_cost": {
    "limit": 100.0,
    "used": 45.5,
    "remaining": 54.5
  }
}
```

### 配额告警

```toml
[llm.quota.alerts]
thresholds = [80, 90, 100]  # 百分比

[80]
action = "log"

[90]
action = "email"
recipients = ["admin@example.com"]

[100]
action = "block"  # 阻止新请求
```

## 限流存储

### 内存存储（MVP）

```rust
// 简单的 HashMap 实现
struct RateLimiter {
    limits: HashMap<String, (u32, Instant)>  // (count, window_start)
}
```

问题：重启丢失数据，单机限制。

### Redis 存储（未来）

```toml
[llm.rate_limit.storage]
type = "redis"
url = "redis://localhost:6379"
```

优点：
- 分布式限流
- 持久化
- 高性能

## 限流监控

### 指标

```bash
GET /metrics/rate_limit

{
  "global": {
    "limit": 100,
    "used": 45,
    "remaining": 55
  },
  "by_ip": {
    "192.168.1.100": {
      "limit": 60,
      "used": 10,
      "remaining": 50
    }
  }
}
```

### 日志

```
[INFO] req-abc123 Rate limit check: 45/100 (global)
[WARN] req-abc124 Rate limit exceeded: 101/100 (global)
[INFO] req-abc125 Rate limit reset (global)
```

## MVP 范围

**第一版本实现**：
- 基础 Gateway 限流（全局）
- 固定窗口算法
- 内存存储
- 429 响应和错误处理
- 限流日志

**暂不实现**：
- IP 限流
- API Key 限流
- 模型限流
- Provider 自动限流
- 配额管理
- Redis 存储
- 高级限流算法
- 分布式限流

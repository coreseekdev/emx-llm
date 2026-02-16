# 安全和认证规范

## 概述

安全规范定义 Gateway 的认证、授权和数据保护机制。

## 认证机制

### MVP 阶段：无认证

第一版本不实现 Gateway 层面的认证，直接转发所有请求。

### 认证层级

```
┌─────────────────────────────────────┐
│  Client → Gateway (Optional Auth)   │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Gateway → Provider (Config Auth)   │
└─────────────────────────────────────┘
```

### Gateway 认证（未来扩展）

#### API Key 方式

```bash
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -H "Authorization: Bearer gateway-api-key-xyz" \
  -d '{"messages": [...]}'
```

配置：
```toml
[llm.auth]
enabled = true
type = "api_key"
keys = ["key1", "key2"]
```

#### JWT Token

```bash
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  -d '{"messages": [...]}'
```

配置：
```toml
[llm.auth]
enabled = true
type = "jwt"
secret = "your-jwt-secret"
algorithm = "HS256"
```

#### OAuth 2.0

支持第三方 OAuth 提供商：
- GitHub
- Google
- 自定义 OAuth Server

### Provider 认证

Gateway 从配置中读取 Provider API Key，自动添加到转发请求中。

| Provider | 认证方式 | Header | 配置路径 |
|---------|---------|--------|---------|
| OpenAI | Bearer Token | `Authorization: Bearer sk-...` | `llm.provider.openai.api_key` |
| Anthropic | API Key | `x-api-key: sk-ant-...` | `llm.provider.anthropic.api_key` |

#### 客户端提供 API Key（可选）

客户端可以选择使用自己的 API Key：

```bash
# 使用客户端自己的 API Key
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -H "x-provider-api-key: sk-client-custom-key" \
  -d '{"messages": [...]}'
```

优先级：
1. 客户端提供的 Key（Header: `x-provider-api-key`）
2. Gateway 配置的 Key

## 授权

### API Key 权限（未来）

每个 API Key 可以配置访问权限：

```toml
[llm.auth.keys]
key1 = { providers = ["openai"], models = ["gpt-4", "gpt-3.5-turbo"] }
key2 = { providers = ["anthropic"], models = ["*"] }  # 所有模型
```

### IP 白名单（未来）

```toml
[llm.auth.ip_whitelist]
allowed = ["192.168.1.0/24", "10.0.0.1"]
```

## 数据保护

### API Key 存储

**MVP 阶段**：
- 明文存储在配置文件
- 用户负责保护配置文件权限
- 日志中自动脱敏

**最佳实践**：
```bash
# 设置配置文件权限
chmod 600 config.toml

# 使用环境变量（推荐生产环境）
export EMX_LLM_OPENAI_API_KEY="sk-..."
```

**未来扩展**：
- 加密存储（使用密钥管理服务）
- 从 HashiCorp Vault 读取
- AWS Secrets Manager 集成

### 敏感信息脱敏

日志中自动隐藏 API Key：

```
# 请求日志
[DEBUG] req-abc123 Forwarding to https://api.openai.com/v1/chat/completions
[DEBUG] req-abc123 Authorization: Bearer sk-...****************

# 配置加载
[INFO] Loaded provider openai with api_key: sk-...****************
```

### TLS/SSL

**MVP 阶段**：仅支持 HTTP（localhost 场景）

**未来扩展**：
```toml
[llm.tls]
enabled = true
cert_file = "/path/to/cert.pem"
key_file = "/path/to/key.pem"
```

## 输入验证

### 路径验证

防止路径遍历攻击：

```
# 拒绝
POST /openai/../../../etc/passwd/chat/completions

# 只允许
POST /openai/gpt-4/chat/completions
```

### 模型名称验证

只允许字母、数字、连字符、点号：

```rust
pub fn validate_model_name(model: &str) -> Result<()> {
    let valid = model.chars().all(|c| {
        c.is_alphanumeric() || c == '-' || c == '.'
    });
    if !valid {
        return Err(Error::InvalidModelName);
    }
    Ok(())
}
```

### 请求大小限制

防止大文件攻击：

```toml
[llm.limits]
max_request_size_mb = 10
max_request_duration_sec = 300
```

## CORS（跨域资源共享）

**MVP 阶段**：不允许跨域请求

**未来扩展**：
```toml
[llm.cors]
enabled = true
allowed_origins = ["https://example.com"]
allowed_methods = ["POST", "GET"]
allowed_headers = ["Content-Type", "Authorization"]
```

## 速率限制

详见 [限流功能规范](./rate-limiting.md)

## 审计日志

### 记录内容

- 认证事件（成功/失败）
- 授权检查（允许/拒绝）
- 敏感操作（配置修改、密钥轮换）

### 日志格式

```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "event_type": "auth_success",
  "client_ip": "192.168.1.100",
  "api_key": "key1",
  "request_id": "req-abc123"
}
```

## 安全最佳实践

### 部署建议

1. **生产环境**：
   - 使用环境变量存储 API Key
   - 启用 HTTPS
   - 配置防火墙规则
   - 限制网络访问

2. **开发环境**：
   - 使用配置文件
   - 不对外开放（localhost）
   - 详细日志记录

3. **测试环境**：
   - 使用 Mock Provider
   - 测试 API Key
   - 隔离网络

### API Key 轮换

**手动轮换**：
1. 在 Provider 平台生成新 Key
2. 更新 Gateway 配置
3. 重启 Gateway
4. 撤销旧 Key

**未来自动化轮换**：
- 定期自动轮换
- 无停机轮换

## MVP 范围

**第一版本实现**：
- 无 Gateway 认证
- Provider 认证（从配置读取）
- 敏感信息脱敏（日志）
- 基础输入验证（路径、模型名称）
- 请求大小限制

**暂不实现**：
- Gateway 认证（API Key/JWT/OAuth）
- TLS/SSL
- CORS
- IP 白名单
- 加密存储
- 审计日志（基础日志已有）

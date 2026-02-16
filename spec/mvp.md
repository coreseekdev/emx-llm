# MVP 最小可行产品规范

## 目标

创建 **emx-gate** 独立应用，实现一个简单的 LLM Gateway，支持：
- 聚合不同 provider 的模型
- model 名称在 request body 中（不在 URL 路径中）
- 每种 provider 有独立的 endpoint（因为 model 不通用）
- 统一配置下动态路由到不同 provider
- 保持现有配置机制不变
- 使用 `/v1/` 前缀以兼容 OpenAI 客户端

## MVP 核心功能

### 必须实现（P0）

#### 1. 基础 HTTP 服务器
- 监听指定地址和端口（默认 `127.0.0.1:8848`）
- 处理 HTTP 请求
- 优雅关闭

#### 2. 路由解析
- 支持三种模型名称格式：
  - 短名称：`gpt-4`
  - 限定名称：`openai.gpt-4`
  - 完全限定名称：`openai.some_provider.gpt-4`
- 配置继承机制
- 模型引用到 provider 的映射

#### 3. Provider 转发
- OpenAI Chat Completions API（非流式）
- Anthropic Messages API（非流式）
- model 在 request body 中
- 保持原生 API 格式（不做转换）
- 使用配置中的 API Key 认证

#### 4. 配置管理
- 复用现有配置系统
- 支持 Gateway 配置（host、port）
- Provider 配置保持不变

#### 5. 错误处理
- 模型不存在：返回类似 OpenAI/Anthropic 的错误格式
- Provider 配置缺失：500
- Provider API 错误：转发原始错误（保留原始错误码和消息）
- 超时处理

#### 6. 基础日志
- 启动/关闭日志
- 请求日志（方法、路径、状态、耗时）
- 错误日志

#### 7. 健康检查
- `/health` 端点
- 返回 Gateway 状态

#### 8. 模型列表
- `/v1/models` 端点
- 返回所有可用模型

#### 9. Provider 列表
- `/v1/providers` 端点
- 返回所有可用 Provider 及其模型

### 暂不实现

#### MVP 之后的特性（P1-P2）
- 流式响应（SSE）
- Gateway 认证
- 负载均衡
- 限流
- 故障转移
- 监控指标
- TLS/SSL
- 配置热加载
- Provider 健康检查

## 功能优先级

### P0（MVP 必须实现）
1. HTTP 服务器框架
2. 路由解析逻辑
3. OpenAI Provider 转发
4. Anthropic Provider 转发
5. 基础错误处理
6. 健康检查端点
7. 模型列表端点

### P1（快速迭代）
1. 流式响应支持
2. 请求/响应日志增强
3. 配置验证
4. Provider 健康检查

### P2（未来扩展）
1. 认证和授权
2. 限流和配额
3. 负载均衡
4. 故障转移
5. 监控和指标
6. TLS 支持

## API 设计（MVP）

### OpenAI 兼容端点

```
POST /v1/chat/completions
```

**请求示例**：
```bash
curl -X POST http://localhost:8848/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai.gpt-4",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**行为**：
1. 从 request body 读取 `model` 字段
2. 解析模型名称 `openai.gpt-4`
3. 查找配置 `llm.provider.openai`
4. 转发到 `https://api.openai.com/v1/chat/completions`
5. 返回原始响应

### Anthropic 兼容端点

```
POST /v1/messages
```

**请求示例**：
```bash
curl -X POST http://localhost:8848/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "anthropic.claude-3-opus-20240229",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**行为**：
1. 从 request body 读取 `model` 字段
2. 解析模型名称 `anthropic.claude-3-opus-20240229`
3. 查找配置 `llm.provider.anthropic`
4. 转发到 `https://api.anthropic.com/v1/messages`
5. 返回原始响应

### 辅助端点

#### 健康检查
```bash
GET /health

{
  "status": "ok",
  "timestamp": "2024-01-15T10:30:45Z"
}
```

#### 模型列表
```bash
GET /v1/models

{
  "object": "list",
  "data": [
    {
      "id": "openai.gpt-4",
      "object": "model",
      "owned_by": "openai",
      "permission": [],
      "created": 1677610602
    },
    {
      "id": "anthropic.claude-3-opus-20240229",
      "object": "model",
      "owned_by": "anthropic",
      "permission": [],
      "created": 1677610602
    }
  ]
}
```

#### Provider 列表
```bash
GET /v1/providers

{
  "object": "list",
  "data": [
    {
      "id": "openai",
      "type": "openai",
      "models": ["gpt-4", "gpt-3.5-turbo"],
      "api_base": "https://api.openai.com/v1"
    },
    {
      "id": "anthropic",
      "type": "anthropic",
      "models": ["claude-3-opus-20240229"],
      "api_base": "https://api.anthropic.com"
    },
    {
      "id": "anthropic.glm",
      "type": "anthropic",
      "models": ["glm-4.5", "glm-5"],
      "api_base": "https://open.bigmodel.cn/api/paas/v4/"
    }
  ]
}
```

## 配置示例（MVP）

```toml
# Gateway 配置
[llm]
host = "127.0.0.1"
port = 8848

# Provider 配置（保持不变）
[llm.provider]
type = "openai"
default = "openai.gpt-4"

[llm.provider.openai]
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4"

# OpenAI 兼容服务（如 Azure OpenAI）
[llm.provider.openai.azure]
api_base = "https://your-resource.openai.azure.com/openai/deployments/your-deployment"
api_key = "..."
model = "gpt-4"

[llm.provider.anthropic]
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-3-opus-20240229"

# GLM（Anthropic 兼容）
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
model = "glm-4.5"
```

## CLI 接口

```bash
# 启动 Gateway
emx-gate

# 指定配置文件
emx-gate --config ./config.toml

# 指定监听地址
emx-gate --host 0.0.0.0 --port 9000

# 验证配置
emx-gate --validate

# 测试配置
emx-gate --test
```

## 技术选型

### HTTP 框架
- **axum**：现代、异步、类型安全（推荐）
- **actix-web**：功能丰富、性能好
- **hyper**：底层、灵活

**选择：axum**
- 基于 Tokio 和 Tower
- 类型安全的路由
- 中间件生态丰富
- 与现有 emx-llm 异步架构一致

### HTTP 客户端
- 复用现有的 `reqwest` 客户端

### 配置管理
- 复用现有的 `emx-config-core` 库

## 项目结构

```
emx-llm/
├── src/
│   ├── lib.rs
│   ├── config.rs           # 现有
│   ├── provider.rs         # 现有
│   ├── client.rs           # 现有
│   ├── message.rs          # 现有
│   └── gate/               # 新增 Gateway 库代码
│       ├── mod.rs          # Gateway 模块
│       ├── server.rs       # HTTP 服务器
│       ├── router.rs       # 路由解析
│       ├── handlers.rs     # 请求处理器
│       └── config.rs       # Gateway 配置
└── emx-gate/
    ├── Cargo.toml          # Gateway 二进制配置
    └── src/
        └── main.rs         # 独立的 Gateway 二进制
```

## 验证标准

### 功能验证
1. 启动 Gateway 成功
2. OpenAI 请求转发成功
3. Anthropic 请求转发成功
4. 模型名称解析正确
5. 错误处理正确
6. 健康检查正常
7. 模型列表正确

### 性能验证
1. 单请求延迟 < 100ms（不含 Provider 时间）
2. 支持并发请求
3. 内存占用稳定

### 集成验证
1. 与现有配置系统兼容
2. 不影响现有 `chat` 和 `test` 命令
3. 配置格式向后兼容

## 风险和限制

### 已知限制
1. 不支持流式响应（MVP 阶段）
2. 不支持请求转换
3. 无认证机制
4. 无限流保护
5. 单机部署（不支持集群）

### 风险缓解
1. 清晰的文档说明限制
2. 日志记录所有请求
3. 配置验证提示
4. 超时保护

## 成功标准

MVP 成功的标准：
1. 可以启动 HTTP 服务器
2. 可以转发 OpenAI 请求
3. 可以转发 Anthropic 请求
4. 可以通过模型名称路由到不同 provider
5. 可以列出所有可用模型
6. 可以健康检查
7. 配置与现有系统兼容
8. 基础错误处理工作正常

达到以上标准后，即可发布 MVP 版本，然后根据反馈迭代优化。

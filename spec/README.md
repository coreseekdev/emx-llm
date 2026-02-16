# LLM Gateway 项目规划总结

## 项目目标

创建 **emx-gate** 独立应用，实现一个简单的 LLM Gateway，具有以下特性：

- 不做 API 格式转换（OpenAI → OpenAI，Claude → Claude）
- 聚合不同 provider 的模型
- model 名称在 request body 中（不在 URL 路径中）
- 每种 provider 有独立的 endpoint（因为 model 不通用）
- 统一配置下动态路由到不同 provider
- 复用现有配置机制
- 使用 `/v1/` 前缀以兼容 OpenAI 客户端

## 文档结构

spec/ 目录包含以下规范文档：

### 核心规范
- **[mvp.md](./mvp.md)** - MVP 最小可行产品定义
- **[development-plan.md](./development-plan.md)** - 开发计划和步骤

### 功能规范
- **[routing.md](./routing.md)** - 路由功能规范（模型名称解析、路由机制）
- **[config.md](./config.md)** - 配置管理规范（分层配置、环境变量）
- **[api.md](./api.md)** - API 接口规范（端点设计、请求格式）

### 高级功能（未来扩展）
- **[monitoring.md](./monitoring.md)** - 监控和日志规范
- **[load-balancing.md](./load-balancing.md)** - 负载均衡和故障转移
- **[security.md](./security.md)** - 安全和认证规范
- **[rate-limiting.md](./rate-limiting.md)** - 限流功能规范

## MVP 功能范围

### 第一版本实现（必须）

1. **基础 HTTP 服务器**
   - 监听指定地址和端口（默认 `127.0.0.1:8848`）
   - 处理 HTTP 请求
   - 优雅关闭

2. **路由解析**
   - 短名称：`gpt-4`
   - 限定名称：`openai.gpt-4`
   - 完全限定名称：`openai.some_provider.gpt-4`

3. **Provider 转发**
   - OpenAI Chat Completions API（非流式）
   - Anthropic Messages API（非流式）
   - model 在 request body 中
   - 保持原生 API 格式
   - 使用 `/v1/` 前缀

4. **配置管理**
   - 复用现有配置系统
   - 支持 Gateway 配置（host、port）
   - Provider 配置保持不变

5. **基础功能**
   - 错误处理（参考 GLM API 格式）
   - 日志记录
   - 健康检查（`/health`）
   - 模型列表（`/v1/models`）
   - Provider 列表（`/v1/providers`）

### 暂不实现（MVP 之后）

- 流式响应（SSE）
- Gateway 认证
- 负载均衡
- 限流
- 故障转移
- 监控指标
- TLS/SSL
- 配置热加载

## 开发计划

### 4 个开发阶段

| 阶段 | 工作量 | 主要任务 |
|-----|--------|---------|
| Phase 0: 准备 | 1-2天 | 依赖添加、目录结构、配置结构、CLI 命令 |
| Phase 1: HTTP 服务器 | 2-3天 | 基础服务器、健康检查、模型列表、日志中间件 |
| Phase 2: 路由解析 | 2-3天 | 模型引用解析、配置查找、客户端创建 |
| Phase 3: Provider 转发 | 3-4天 | OpenAI 处理器、Anthropic 处理器、错误处理 |
| Phase 4: 集成测试 | 2-3天 | 配置验证、集成测试、文档编写 |
| **总计** | **10-15天** | MVP 可发布 |

## 技术选型

### HTTP 框架
选择 **axum**：
- 现代异步框架
- 类型安全的路由
- 与现有 Tokio 架构一致
- 丰富的中间件生态

### 其他依赖
- **reqwest**：HTTP 客户端（复用现有）
- **emx-config-core**：配置管理（复用现有）
- **tokio**：异步运行时（复用现有）

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
    └── src/
        └── main.rs         # 独立的 Gateway 二进制
```

## API 设计示例

### OpenAI 兼容端点
```bash
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
```bash
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
```bash
GET /health              # 健康检查
GET /v1/models           # 模型列表
GET /v1/providers        # Provider 列表
```

## 配置示例

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

## CLI 使用

```bash
# 启动 Gateway
emx-gate

# 指定配置文件
emx-gate --config ./config.toml

# 指定监听地址
emx-gate --host 0.0.0.0 --port 9000

# 验证配置
emx-gate --validate
```

## 下一步行动

1. **审阅规范文档**：确认 MVP 范围和技术方案
2. **开始 Phase 0**：添加依赖、创建目录结构
3. **逐步实现**：按照开发计划逐个阶段完成
4. **持续测试**：每个阶段完成后进行验证

## 成功标准

MVP 发布标准：
- [ ] 可以启动 HTTP 服务器
- [ ] 可以转发 OpenAI 请求
- [ ] 可以转发 Anthropic 请求
- [ ] 可以通过模型名称路由到不同 provider
- [ ] 可以列出所有可用模型
- [ ] 可以健康检查
- [ ] 配置与现有系统兼容
- [ ] 基础错误处理工作正常

## 参考资料

### 分析报告
1. **emx-llm 项目分析**：现有代码库结构、配置机制
2. **bifrost 分析**：统一接口、流式处理、分层配置
3. **llmgateway 分析**：智能路由、负载均衡、限流机制

### 相关项目
- emx-llm：现有 LLM 客户端库
- bifrost：目录下的另一个 LLM gateway
- llmgateway：目录下的另一个 LLM gateway

## 联系方式

如有问题或需要澄清，请及时沟通确认。

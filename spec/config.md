# 配置管理规范

## 概述

LLM Gateway 需要一个灵活、分层的配置系统，支持多 provider、多模型的配置管理。

## 配置层次

### 优先级（从高到低）

1. **环境变量**
   - `EMX_LLM_*` 前缀
   - 兼容传统变量（`OPENAI_API_KEY`, `ANTHROPIC_AUTH_TOKEN`）

2. **本地配置文件**
   - `./config.toml`
   - 项目级配置

3. **全局配置文件**
   - `~/.emx/config.toml`
   - 用户级配置

4. **默认值**
   - 内置常量

## 配置结构

### 完整配置示例

```toml
[llm]
# Gateway 全局配置
host = "127.0.0.1"
port = 8080
log_level = "info"

[llm.provider]
# 默认 provider 设置
type = "openai"
default = "openai.gpt-4"

# OpenAI 配置段
[llm.provider.openai]
type = "openai"
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4"
max_tokens = 4096
timeout = 60

# OpenAI 生产环境配置
[llm.provider.openai.production]
api_base = "https://api.openai.com/v1"
api_key = "sk-prod-..."
model = "gpt-4-turbo"

# Anthropic 配置段
[llm.provider.anthropic]
type = "anthropic"
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-3-opus-20240229"
max_tokens = 4096
timeout = 60

# GLM（使用 Anthropic 协议）
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
model = "glm-4.5"
max_tokens = 8192

# GLM 特定模型
[llm.provider.anthropic.glm.glm-5]
model = "glm-5"
max_tokens = 16384
```

## 配置参数说明

### Gateway 全局配置

| 参数 | 类型 | 默认值 | 说明 |
|-----|------|--------|------|
| `host` | String | `127.0.0.1` | 监听地址 |
| `port` | Integer | `8080` | 监听端口 |
| `log_level` | String | `info` | 日志级别（debug/info/warn/error） |

### Provider 配置参数

| 参数 | 类型 | 必需 | 说明 |
|-----|------|------|------|
| `type` | String | 是 | Provider 类型（openai/anthropic） |
| `api_base` | String | 是 | API 基础 URL |
| `api_key` | String | 是 | API 密钥 |
| `model` | String | 是 | 默认模型名称 |
| `max_tokens` | Integer | 否 | 最大 token 数（默认 4096） |
| `timeout` | Integer | 否 | 请求超时秒数（默认 60） |

## 配置加载机制

### 环境变量映射

| 环境变量 | 配置路径 | 说明 |
|---------|---------|------|
| `EMX_LLM_HOST` | `llm.host` | 监听地址 |
| `EMX_LLM_PORT` | `llm.port` | 监听端口 |
| `EMX_LLM_PROVIDER_TYPE` | `llm.provider.type` | 默认 provider |
| `EMX_LLM_OPENAI_API_KEY` | `llm.provider.openai.api_key` | OpenAI API Key |
| `EMX_LLM_ANTHROPIC_API_KEY` | `llm.provider.anthropic.api_key` | Anthropic API Key |
| `OPENAI_API_KEY` | `llm.provider.openai.api_key` | 传统兼容 |
| `ANTHROPIC_AUTH_TOKEN` | `llm.provider.anthropic.api_key` | 传统兼容 |

### 配置验证

启动时验证：
1. 所有必需的配置项存在
2. API 格式有效（URL、Key）
3. 端口可用
4. 配置段无循环引用

运行时验证：
1. 模型引用可解析
2. Provider 可访问

## 配置热加载

**MVP 阶段**：不支持热加载，需要重启 gateway

**未来扩展**：
- 监听配置文件变化
- SIGHUP 信号重载
- API 端点触发重载

## 配置测试

提供 CLI 命令测试配置：

```bash
# 测试所有 provider 配置
emx-llm gate --test-config

# 测试特定 provider
emx-llm gate --test-provider openai

# 验证模型引用
emx-llm gate --validate-model "anthropic.glm.glm-5"
```

## 敏感信息处理

### API Key 安全
- 配置文件中的 API Key 明文存储（用户责任）
- 支持从环境变量读取（推荐生产环境）
- 日志中隐藏 API Key（脱敏）

### 配置文件权限
- 建议配置文件权限 `600`（仅所有者读写）
- 启动时检查文件权限（警告）

## MVP 范围

**第一版本实现**：
- 分层配置加载（环境变量、本地、全局、默认）
- TOML 格式配置文件
- 配置继承机制
- 基础配置验证
- 环境变量映射

**暂不实现**：
- 配置热加载
- 加密存储
- 配置版本管理
- 远程配置中心

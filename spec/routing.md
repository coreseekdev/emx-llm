# 路由功能规范

## 概述

LLM Gateway 的核心功能是将客户端请求路由到不同的 LLM Provider。路由机制需要支持模型名称解析、智能选择和故障转移。

## 功能需求

### 1. 模型名称格式

支持三种模型引用格式：
- **短名称**：`model-name` - 简洁形式，需要在配置中唯一
- **限定名称**：`provider.model-name` - 指定提供商
- **完全限定名称**：`provider.sub-provider.model-name` - 完整路径

示例：
```
gpt-4                      # 短名称
openai.gpt-4               # 限定名称
openai.production.gpt-4    # 完全限定名称
anthropic.glm.glm-5        # 完全限定（第三方）
```

### 2. 路由解析机制

#### 解析策略
1. **精确匹配**：优先查找完全匹配的配置段
2. **向上查找**：未找到时向父级查找继承配置
3. **默认值**：使用配置的默认模型或提供商

#### 解析流程
```
输入: "anthropic.glm.glm-5"
  ↓
查找: [llm.provider.anthropic.glm.glm-5]
  ↓ 未找到
查找: [llm.provider.anthropic.glm]
  ↓ 未找到
查找: [llm.provider.anthropic]
  ↓ 找到基础配置
合并: glm-5 特定配置 + anthropic 基础配置
  ↓
输出: Provider (Anthropic) + Model (glm-5) + API Base/Key
```

### 3. Provider 类型映射

| Provider Type | API 兼容性 | 默认 Base URL |
|--------------|-----------|---------------|
| `openai` | OpenAI API | `https://api.openai.com/v1` |
| `anthropic` | Anthropic API | `https://api.anthropic.com` |

支持自定义 Base URL，用于：
- 第三方兼容服务（如 GLM、通义千问）
- 代理或网关服务
- 内部部署的模型服务

### 4. 路由决策

#### 基础路由
- 根据模型名称中的 provider 部分选择客户端
- 从配置中读取 API Base、API Key、Model 等参数
- 验证必需的配置项存在

#### 高级路由（未来扩展）
- **优先级路由**：不同 provider 设置权重
- **成本优化**：根据 token 价格选择最优 provider
- **性能路由**：根据响应时间选择最快 provider
- **地域路由**：根据用户位置选择最近的 provider

## 配置示例

```toml
[llm.provider]
type = "openai"              # 默认 provider 类型
default = "openai.gpt-4"     # 默认模型

# OpenAI 官方
[llm.provider.openai]
api_base = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4"

# Anthropic 官方
[llm.provider.anthropic]
api_base = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-3-opus-20240229"

# GLM（Anthropic 兼容）
[llm.provider.anthropic.glm]
api_base = "https://open.bigmodel.cn/api/paas/v4/"
api_key = "..."
model = "glm-4.5"

# GLM 特定模型
[llm.provider.anthropic.glm.glm-5]
model = "glm-5"              # 继承 api_base 和 api_key
```

## API 设计

### 客户端创建
```rust
// 基础方式
let client = create_client(config)?;

// 基于模型引用
let client = create_client_for_model(config, "anthropic.glm.glm-5")?;
```

### 模型解析
```rust
pub struct ModelReference {
    pub provider_type: ProviderType,
    pub model_path: Vec<String>,  // e.g. ["glm", "glm-5"]
    pub model_name: String,        // e.g. "glm-5"
}

pub fn parse_model_reference(model: &str) -> Result<ModelReference>;
```

## 测试场景

### 正常场景
1. 使用短名称查询模型（配置中唯一）
2. 使用限定名称查询模型
3. 使用完全限定名称查询模型
4. 继承父级配置

### 异常场景
1. 模型名称不存在
2. Provider 配置缺失（如缺少 API Key）
3. 不支持的 Provider 类型
4. 循环引用检测

## MVP 范围

**第一版本实现**：
- 三种模型名称格式支持
- 配置继承机制
- OpenAI 和 Anthropic 两种 provider 类型
- 基础错误处理

**暂不实现**：
- 优先级路由
- 成本优化
- 性能监控
- 自动故障转移

# Tool Call 功能设计

## 1. 概述

基于 `rtcl` 实现可扩展的工具调用（Tool Call）系统。每个工具对应一个 TCL 脚本，脚本内置工具的元数据定义。系统支持 OpenAI 和 Anthropic 两种不同的工具定义格式。

## 2. 架构设计

### 2.1 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                    emx-llm CLI                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌───────────────┐      ┌──────────────┐                  │
│  │ Tool Manager  │─────▶│   rtcl VM    │                  │
│  └───────────────┘      └──────────────┘                  │
│         │                       │                            │
│         │                       ▼                            │
│         │              ┌──────────────┐                   │
│         │              │ tools/*.tcl  │                   │
│         │              └──────────────┘                   │
│         │                                                    │
│         ▼                                                    │
│  ┌─────────────────────────────────────┐                   │
│  │    Tool Registry (OpenAI/Anthropic)  │                   │
│  └─────────────────────────────────────┘                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 目录结构

```
emx-llm/
├── Cargo.toml
├── third_party/
│   └── rtcl/                    # Git submodule: G:/src.tcl/rtcl
├── tools/                       # 工具定义目录
│   ├── README.md                # 工具开发指南
│   ├── read.tcl                 # 默认工具：读取文件内容
│   └── glob.tcl                 # 默认工具：文件路径匹配
├── src/
│   ├── tool/
│   │   ├── mod.rs               # Tool 模块入口
│   │   ├── registry.rs          # 工具注册表
│   │   ├── manager.rs           # 工具管理器
│   │   ├── executor.rs          # TCL 执行器
│   │   └── schema.rs            # Schema 转换 (OpenAI/Anthropic)
│   └── bin/emx-llm/
│       └── commands.rs          # CLI 命令定义
└── docs/
    └── tool-call-design.md      # 本文档
```

## 3. TCL 工具定义格式

### 3.1 工具脚本结构

每个 TCL 工具脚本必须实现一个 `info` 命令来返回工具元数据：

```tcl
# tools/read.tcl

# ===== 工具信息（通过 info 命令返回） =====
proc info {} {
    return [dict create \
        name "read" \
        description "Read the contents of a file from the filesystem" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The file path to read" \
            ] \
        ] \
        returns "The file contents as a string" \
        example "read /path/to/file.txt"
    ]
}

# ===== 工具实现 =====
proc execute {args} {
    # 解析参数
    set path [lindex $args 0]

    # 检查参数
    if {$path eq ""} {
        error "Missing required parameter: path"
    }

    # 执行工具逻辑
    set fp [open $path r]
    set content [read $fp]
    close $fp

    # 返回结果（JSON 格式）
    return [json_write {
        content $content
    }]
}

# JSON 辅助函数
proc json_write {dict_val} {
    return [dict_to_json $dict_val]
}
```

### 3.2 元数据格式

通过 `info` 命令返回的字典结构：

| 键 | 类型 | 必填 | 说明 |
|---|------|------|------|
| `name` | string | ✅ | 工具名称，唯一标识符 |
| `description` | string | ✅ | 工具功能描述 |
| `parameters` | dict | ✅ | 参数字典，键为参数名 |
| `returns` | string | ❌ | 返回值描述 |
| `example` | string | ❌ | 使用示例 |

参数字典中每个参数的结构：

| 键 | 类型 | 必填 | 说明 |
|---|------|------|------|
| `type` | string | ✅ | 参数类型 |
| `required` | boolean | ✅ | 是否必填 |
| `description` | string | ✅ | 参数描述 |

### 3.3 参数类型支持

| 类型 | 说明 | 示例 |
|------|------|------|
| `string` | 字符串 | `"file.txt"` |
| `integer` | 整数 | `42` |
| `number` | 浮点数 | `3.14` |
| `boolean` | 布尔值 | `true` / `false` |
| `array` | 数组 | `["a", "b", "c"]` |
| `object` | 对象 | `{"key": "value"}` |

### 3.4 默认工具集

emx-llm 默认提供两个基础工具：

| 工具 | 功能 |
|------|------|
| **read** | 读取文件内容 |
| **glob** | 文件路径匹配（通配符搜索） |

其他工具（如 write、list_files 等）由用户根据需要自行添加。

#### tools/read.tcl

```tcl
# 工具信息
proc info {} {
    return [dict create \
        name "read" \
        description "Read the contents of a file" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The file path to read" \
            ] \
        ] \
        returns "The file contents as a string" \
        example "read /path/to/file.txt"
    ]
}

# 工具实现
proc execute {args} {
    set path [lindex $args 0]

    if {$path eq ""} {
        error "Missing required parameter: path"
    }

    set fp [open $path r]
    set content [read $fp]
    close $fp

    return [dict create content $content]
}
```

#### tools/glob.tcl

```tcl
# 工具信息
proc info {} {
    return [dict create \
        name "glob" \
        description "Find files matching a pattern" \
        parameters [dict create \
            pattern [dict create \
                type "string" \
                required true \
                description "The glob pattern (e.g., *.rs, **/*.txt)" \
            ] \
            path [dict create \
                type "string" \
                required false \
                description "Base directory to search (default: current directory)" \
            ] \
        ] \
        returns "List of matching file paths" \
        example "glob **/*.md src"
}

# 工具实现
proc execute {args} {
    set pattern [lindex $args 0]
    set path [expr {[llength $args] > 1 ? [lindex $args 1] : "."}]

    if {$pattern eq ""} {
        error "Missing required parameter: pattern"
    }

    set matches [glob -nocomplain -directory $path -- $pattern]

    return [dict create matches $matches]
}
```

## 4. Rust 集成

### 4.1 依赖配置

**Cargo.toml**

```toml
[dependencies]
# rtcl - 本地 git submodule
rtcl = { path = "third_party/rtcl/crates/rtcl-core" }

# JSON 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 异步运行时
tokio = { version = "1", features = ["full"] }
```

### 4.2 核心模块设计

#### src/tool/mod.rs

```rust
//! Tool call 功能模块
//!
//! 基于 rtcl 实现可扩展的工具调用系统

pub mod executor;
pub mod manager;
pub mod registry;
pub mod schema;

pub use executor::TclExecutor;
pub use manager::ToolManager;
pub use registry::ToolRegistry;
pub use schema::{ToolSchema, ToolParameter, OpenAIFormat, AnthropicFormat};

use std::path::PathBuf;

/// 工具配置
pub struct ToolConfig {
    /// 工具定义目录
    pub tools_dir: PathBuf,
    /// 启用的工具列表
    pub enabled_tools: Vec<String>,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            tools_dir: PathBuf::from("tools"),
            // 默认提供基础的读取和搜索工具
            enabled_tools: vec!["read".to_string(), "glob".to_string()],
        }
    }
}
```

#### src/tool/executor.rs

```rust
//! TCL 工具执行器

use rtcl::Interp;
use anyhow::{Result, Context};

/// TCL 工具执行器
pub struct TclExecutor {
    interp: Interp,
}

impl TclExecutor {
    /// 创建新的执行器
    pub fn new() -> Result<Self> {
        let mut interp = Interp::new();

        // 注册内置辅助函数（dict_to_json 等）
        register_helpers(&mut interp)?;

        Ok(Self { interp })
    }

    /// 加载工具脚本
    pub fn load_tool(&mut self, script_path: &PathBuf) -> Result<()> {
        let script = std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read tool script: {:?}", script_path))?;

        self.interp.eval(&script)
            .with_context(|| format!("Failed to load tool script: {:?}", script_path))?;

        Ok(())
    }

    /// 执行工具
    pub fn execute(&mut self, tool_name: &str, args: Vec<String>) -> Result<String> {
        let cmd = format!("execute {}", args.join(" "));

        let result = self.interp.eval(&cmd)
            .with_context(|| format!("Tool execution failed: {}", tool_name))?;

        Ok(result.as_str().to_string())
    }

    /// 通过调用 info 命令获取工具元数据
    pub fn get_metadata(&mut self, tool_name: &str) -> Result<ToolMetadata> {
        // 调用工具的 info 命令
        let cmd = format!("info");
        let result = self.interp.eval(&cmd)
            .with_context(|| format!("Failed to get info for tool: {}", tool_name))?;

        // 解析返回的字典（Tcl dict 格式）
        let dict_str = result.as_str();
        let metadata = parse_tcl_dict(dict_str)?;

        Ok(ToolMetadata {
            name: metadata.get("name").unwrap_or(&tool_name.to_string()).to_string(),
            description: metadata.get("description").unwrap_or(&String::new()).to_string(),
            parameters: parse_parameters(metadata.get("parameters")),
        })
    }
}

/// 解析 Tcl 字典格式
fn parse_tcl_dict(dict_str: &str) -> Result<HashMap<String, String>> {
    // 简化实现：解析 Tcl 的 dict create 输出
    // 实际实现需要更复杂的解析逻辑
    // ...
}

/// 解析参数字典
fn parse_parameters(params_opt: Option<&String>) -> Vec<ToolParameter> {
    // 解析 Tcl dict 中的 parameters 字段
    // ...
    vec![]
}

/// 工具元数据
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
}

#[derive(Debug, Clone)]
pub struct ToolParameter {
    pub name: String,
    pub type_: String,
    pub required: bool,
    pub description: String,
}
```

#### src/tool/registry.rs

```rust
//! 工具注册表 - 管理 OpenAI/Anthropic 格式的工具定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, ToolMetadata>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, metadata: ToolMetadata) {
        self.tools.insert(metadata.name.clone(), metadata);
    }

    /// 生成 OpenAI 格式的工具定义
    pub fn to_openai_format(&self) -> Vec<OpenAITool> {
        self.tools.values()
            .map(|meta| OpenAITool::from(meta.clone()))
            .collect()
    }

    /// 生成 Anthropic 格式的工具定义
    pub fn to_anthropic_format(&self) -> Vec<AnthropicTool> {
        self.tools.values()
            .map(|meta| AnthropicTool::from(meta.clone()))
            .collect()
    }

    /// 获取工具定义
    pub fn get(&self, name: &str) -> Option<&ToolMetadata> {
        self.tools.get(name)
    }

    /// 列出所有工具
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

/// OpenAI 工具定义格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub type_: String,  // "function"
    pub function: OpenAIFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub description: String,
    pub parameters: OpenAIParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIParameters {
    #[serde(rename = "type")]
    pub type_: String,  // "object"
    pub properties: HashMap<String, OpenAIProperty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIProperty {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Anthropic 工具定义格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    pub description: String,
    pub input_schema: AnthropicSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicSchema {
    #[serde(rename = "type")]
    pub type_: String,  // "object"
    pub properties: HashMap<String, AnthropicProperty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicProperty {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<ToolMetadata> for OpenAITool {
    fn from(meta: ToolMetadata) -> Self {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &meta.parameters {
            properties.insert(
                param.name.clone(),
                OpenAIProperty {
                    type_: param.type_.clone(),
                    description: Some(param.description.clone()),
                },
            );

            if param.required {
                required.push(param.name.clone());
            }
        }

        OpenAITool {
            type_: "function".to_string(),
            function: OpenAIFunction {
                name: meta.name,
                description: meta.description,
                parameters: OpenAIParameters {
                    type_: "object".to_string(),
                    properties,
                    required: if required.is_empty() { None } else { Some(required) },
                },
            },
        }
    }
}

impl From<ToolMetadata> for AnthropicTool {
    fn from(meta: ToolMetadata) -> Self {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &meta.parameters {
            properties.insert(
                param.name.clone(),
                AnthropicProperty {
                    type_: param.type_.clone(),
                    description: Some(param.description.clone()),
                },
            );

            if param.required {
                required.push(param.name.clone());
            }
        }

        AnthropicTool {
            name: meta.name,
            description: meta.description,
            input_schema: AnthropicSchema {
                type_: "object".to_string(),
                properties,
                required: if required.is_empty() { None } else { Some(required) },
            },
        }
    }
}
```

#### src/tool/manager.rs

```rust
//! 工具管理器

use crate::tool::{ToolConfig, TclExecutor, ToolRegistry};
use anyhow::Result;
use std::path::PathBuf;

/// 工具管理器
pub struct ToolManager {
    executor: TclExecutor,
    registry: ToolRegistry,
    config: ToolConfig,
}

impl ToolManager {
    /// 从配置创建工具管理器
    pub fn from_config(config: ToolConfig) -> Result<Self> {
        let mut executor = TclExecutor::new()?;
        let mut registry = ToolRegistry::new();

        // 扫描工具目录
        let tools_dir = &config.tools_dir;

        if tools_dir.exists() {
            for entry in std::fs::read_dir(tools_dir)? {
                let entry = entry?;
                let path = entry.path();

                // 只处理 .tcl 文件
                if path.extension().and_then(|s| s.to_str()) == Some("tcl") {
                    // 提取元数据
                    let metadata = executor.extract_metadata(&path)?;

                    // 只加载启用的工具
                    if config.enabled_tools.contains(&metadata.name) {
                        executor.load_tool(&path)?;
                        registry.register(metadata);
                    }
                }
            }
        }

        Ok(Self {
            executor,
            registry,
            config,
        })
    }

    /// 获取 OpenAI 格式的工具定义
    pub fn openai_tools(&self) -> Vec<serde_json::Value> {
        self.registry.to_openai_format()
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect()
    }

    /// 获取 Anthropic 格式的工具定义
    pub fn anthropic_tools(&self) -> Vec<serde_json::Value> {
        self.registry.to_anthropic_format()
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect()
    }

    /// 执行工具调用
    pub async fn execute_tool(&mut self, name: &str, args: Vec<String>) -> Result<String> {
        self.executor.execute(name, args)
    }
}
```

## 5. CLI 集成

### 5.1 新增命令

```bash
# 列出可用工具
emx-llm tools list

# 显示工具详情（调用工具的 info 命令）
emx-llm tools show <tool_name>

# 验证工具定义
emx-llm tools validate [tool_name]

# 查看工具的帮助信息
emx-llm tools help <tool_name>
```

### 5.2 Chat 命令集成

```bash
# 启用默认工具集（只有 read）
emx-llm chat my-session --tools "What's in src/main.rs?"

# 启用指定工具
emx-llm chat my-session --tools read "Read the config file"
```

## 6. 工具调用流程

### 6.1 执行流程

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. LLM 返回 tool_call 请求                                     │
│    {                                                          │
│      "tool_calls": [                                          │
│        {                                                      │
│          "id": "call_abc123",                                 │
│          "function": { "name": "read", "arguments": {...} }   │
│        }                                                      │
│      ]                                                        │
│    }                                                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. 解析工具调用                                                │
│    - 提取工具名称: "read"                                       │
│    - 解析参数: {"path": "/path/to/file"}                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. 查找工具定义                                                │
│    - 检查工具是否已注册                                         │
│    - 验证参数类型和必填项                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. 执行 TCL 脚本                                               │
│    - 加载 tools/read.tcl                                       │
│    - 调用 tool_main 函数                                        │
│    - 传入解析后的参数                                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. 处理执行结果                                                │
│    - 解析 TCL 返回的 JSON                                      │
│    - 构造 tool_response 消息                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. 继续对话                                                    │
│    - 将 tool_response 发送给 LLM                                │
│    - 等待 LLM 最终响应                                          │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 错误处理

```rust
pub enum ToolError {
    /// 工具未注册
    ToolNotFound(String),

    /// 参数验证失败
    ParameterValidation(String),

    /// TCL 执行错误
    TclExecution(String),

    /// 返回值解析失败
    ResultParse(String),

    /// 超时
    Timeout(Duration),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ToolNotFound(name) => write!(f, "Tool not found: {}", name),
            ToolError::ParameterValidation(msg) => write!(f, "Parameter validation failed: {}", msg),
            ToolError::TclExecution(msg) => write!(f, "TCL execution error: {}", msg),
            ToolError::ResultParse(msg) => write!(f, "Failed to parse tool result: {}", msg),
            ToolError::Timeout(d) => write!(f, "Tool execution timeout after {:?}", d),
        }
    }
}
```

## 7. 安全考虑

### 7.1 沙箱机制

- 工具脚本运行在受限环境中
- 禁止访问系统命令
- 限制文件系统访问范围
- 设置执行超时

### 7.2 路径验证

```tcl
proc validate_path {path {base_dir "."}} {
    # 检查路径是否在允许的目录内
    set abs_path [file normalize $path]
    set abs_base [file normalize $base_dir]

    if {![string match "${abs_base}*" $abs_path]} {
        error "Path outside allowed directory: $path"
    }

    return $abs_path
}
```

### 7.3 资源限制

- 单个工具执行超时：30 秒
- 返回值大小限制：1MB
- 并发执行限制：最多 3 个工具

## 8. 测试策略

### 8.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registration() {
        let mut registry = ToolRegistry::new();
        let metadata = ToolMetadata {
            name: "test".to_string(),
            description: "Test tool".to_string(),
            parameters: vec![],
        };

        registry.register(metadata);
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn test_openai_format() {
        let metadata = ToolMetadata {
            name: "read".to_string(),
            description: "Read file".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "path".to_string(),
                    type_: "string".to_string(),
                    required: true,
                    description: "File path".to_string(),
                },
            ],
        };

        let openai_tool: OpenAITool = metadata.into();
        assert_eq!(openai_tool.function.name, "read");
    }
}
```

### 8.2 集成测试

```rust
#[tokio::test]
async fn test_tool_execution() {
    let mut manager = ToolManager::from_config(ToolConfig::default()).unwrap();

    // 测试 read 工具
    let result = manager.execute_tool("read", vec!["test.txt".to_string()]).await;
    assert!(result.is_ok());
}
```

### 8.3 TCL 脚本测试

```tcl
# tests/tool_tests.tcl

# 测试参数解析
proc test_parse_params {} {
    set result [parse_params {"a" "b"} {
        {x "" true}
        {y "default" false}
    }]

    # 验证结果
    if {[dict get $result x] ne "a"} {
        error "Parameter parsing failed"
    }
}

# 运行测试
test_parse_params
puts "All tests passed"
```

## 9. 部署和分发

### 9.1 Git Submodule 配置

```bash
# 添加 submodule
cd emx-llm
git submodule add G:/src.tcl/rtcl third_party/rtcl

# 更新 submodule
git submodule update --remote third_party/rtcl

# 初始化 submodule（克隆后）
git submodule update --init --recursive
```

### 9.2 构建配置

**.cargo/config.toml**

```toml
[build]
# 为 rtcl 启用必要的 features
```

**Cargo.toml**

```toml
[dependencies]
rtcl = { path = "third_party/rtcl/crates/rtcl-core", features = ["std", "file"] }
```

## 10. 未来扩展

- [ ] 工具版本管理
- [ ] 工具依赖管理（工具调用其他工具）
- [ ] 异步工具执行（长时间运行的工具）
- [ ] 工具执行历史记录
- [ ] 工具性能监控
- [ ] 自定义工具类型定义
- [ ] 工具权限管理（某些工具需要额外授权）
- [ ] 工具市场（用户分享和下载工具）

## 11. 参考文档

- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling)
- [Anthropic Tool Use](https://docs.anthropic.com/claude/docs/tool-use)
- [rtcl Documentation](https://github.com/rtcl-project/rtcl)
- [Tcl Syntax](https://www.tcl.tk/man/tcl/TclLib/Tcl.htm)

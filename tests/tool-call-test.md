# Tool Call 测试场景

## 测试目标

1. ✅ 验证能正确发出 tool call 调用
2. ✅ 验证能正确处理一次发出的多个 tool call 调用
3. ✅ 验证能以正确的格式将结果返回给 llm api
4. ✅ 验证当没有 tool call 时，能正确停止推理

## 测试前提条件

- emx-llm 已编译并可用
- 已配置有效的 API key（OpenAI 兼容 API）
- tools/ 目录中有 glob.tcl 和 read.tcl 工具

## 测试场景

### 场景 1: 单个 tool call

**Prompt:**
```
查找当前目录下所有的 .md 文件
```

**期望行为:**
1. LLM 返回 tool call: `glob(pattern="**/*.md", path=".")`
2. 系统执行 glob 工具
3. 将结果返回给 LLM
4. LLM 返回格式化的文件列表

**命令:**
```bash
cd E:/src.llm/emx-llm
./target/release/emx-llm chat test-single --no-stream --tools ./tools --raw "查找当前目录下所有的 .md 文件"
```

### 场景 2: 多个 tool calls

**Prompt:**
```
先查找所有的 .rs 文件，然后读取 Cargo.toml 文件的内容
```

**期望行为:**
1. LLM 返回两个 tool calls:
   - `glob(pattern="**/*.rs", path=".")`
   - `read(path="Cargo.toml")`
2. 系统依次执行两个工具
3. 将两个工具的结果都返回给 LLM
4. LLM 综合两个工具的结果给出响应

**命令:**
```bash
cd E:/src.llm/emx-llm
./target/release/emx-llm chat test-multiple --no-stream --tools ./tools --raw "先查找所有的 .rs 文件，然后读取 Cargo.toml 文件的内容"
```

### 场景 3: 无 tool call（正常对话）

**Prompt:**
```
你好，请介绍一下你自己
```

**期望行为:**
1. LLM 直接返回文本响应
2. 不执行任何工具调用
3. 正常结束对话

**命令:**
```bash
cd E:/src.llm/emx-llm
./target/release/emx-llm chat test-normal --no-stream "你好，请介绍一下你自己"
```

### 场景 4: Tool call 错误处理

**Prompt:**
```
调用一个不存在的工具 mytool
```

**期望行为:**
1. LLM 尝试调用 my_tool（如果它认为这是一个工具）
2. 系统报告工具未找到
3. 将错误信息返回给 LLM
4. LLM 解释错误或建议替代方案

**命令:**
```bash
cd E:/src.llm/emx-llm
./target/release/emx-llm chat test-error --no-stream --tools ./tools --raw "使用 my_tool 工具执行某个操作"
```

## 预期输出格式

### Tool Call 检测输出
```
[Tool Calls: 2]
  [1] glob: {"pattern": "**/*.rs", "path": "."}
  [2] read: {"path": "Cargo.toml"}
[Executed: glob]
[Executed: read]
```

### Tool Result 输出（使用 --raw）
```
[Tool Calls: 1]
  [1] glob: {"pattern": "**/*.md", "path": "."}

[Tool Result: glob]
README.md
CLAUDE.md
TODO.md
...
```

### 正常对话输出（无 tool calls）
```
你好！我是一个 AI 助手，可以帮助你...
```

## 验证检查点

- [x] 代码编译通过
- [x] 工具定义加载功能已实现
- [x] OpenAI 工具调用支持已完成
- [x] Anthropic 工具调用支持已完成
- [x] 流式响应工具调用支持已完成
- [x] Session 工具调用存储已完成
- [ ] 场景 1: 单个 tool call 执行成功（需要有效 API 密钥）
- [ ] 场景 2: 多个 tool calls 依次执行（需要有效 API 密钥）
- [ ] 场景 3: 无 tool call 时正常对话（需要有效 API 密钥）
- [ ] 场景 4: 错误处理正确（需要有效 API 密钥）

## 可用工具

### glob
- **描述**: 查找匹配模式的文件
- **参数**:
  - `pattern` (必需): glob 模式（如 `*.rs`, `**/*.txt`）
  - `path` (可选): 基础目录（默认当前目录）

### read
- **描述**: 读取文件内容
- **参数**:
  - `path` (必需): 文件路径

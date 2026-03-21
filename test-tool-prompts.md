# Tool Call 测试 Prompt 集合

本文档包含用于测试 emx-llm 工具调用功能的各种 prompt。

## 测试场景 1: 单个工具调用

**目的**: 测试 LLM 能否正确识别需要使用工具并发出单个 tool call

```
请读取当前目录下的 Cargo.toml 文件的内容，并告诉我项目的名称。
```

**预期行为**:
1. LLM 识别需要读取文件
2. 调用 `read` 工具，参数 `path: "Cargo.toml"`
3. 接收文件内容
4. 解析并回答项目名称

---

## 测试场景 2: 多个工具调用（并行）

**目的**: 测试 LLM 能否在一次响应中发出多个 tool call

```
请同时帮我做以下事情：
1. 读取 src/lib.rs 文件的内容
2. 列出所有 .rs 文件

然后告诉我有多少个 Rust 源文件，以及 lib.rs 的前 50 个字符。
```

**预期行为**:
1. LLM 识别需要两个独立的操作
2. 在同一响应中调用两个工具：
   - `read` with `path: "src/lib.rs"`
   - `glob` with `pattern: "*.rs"`
3. 并行接收两个结果
4. 综合回答问题

---

## 测试场景 3: 串行工具调用

**目的**: 测试工具调用的链式处理（基于第一个工具结果决定第二个工具）

```
请列出所有 .md 文件，然后读取第一个 markdown 文件的内容，告诉我它的标题是什么。
```

**预期行为**:
1. 第一轮：调用 `glob` with `pattern: "*.md"`
2. 接收文件列表
3. 第二轮：基于列表调用 `read` 读取第一个文件
4. 解析并提取标题

---

## 测试场景 4: 无工具调用

**目的**: 测试当不需要工具时，LLM 能否直接回答并正确停止推理

```
请用一句话解释什么是 Rust 编程语言。
```

**预期行为**:
1. LLM 识别这是一个知识性问题
2. 不调用任何工具
3. 直接返回文本答案
4. 正确结束推理

---

## 测试场景 5: 工具调用错误处理

**目的**: 测试当工具调用失败时的处理

```
请读取一个名为 nonexistent.txt 的文件，然后告诉我文件内容。
```

**预期行为**:
1. LLM 尝试调用 `read` 工具
2. 工具返回错误信息
3. LLM 根据错误信息给出适当的回复

---

## 测试场景 6: 复杂多轮工具调用

**目的**: 测试复杂的工具调用场景

```
请分析当前项目的 Rust 源代码结构：
1. 首先找出所有 .rs 文件
2. 读取 src/lib.rs 的内容
3. 检查是否使用了 async/await
4. 列出主要依赖的外部 crate

给我一个简洁的总结。
```

**预期行为**:
1. 第一轮：调用 `glob` 和 `read`（并行）
2. 第二轮：基于 lib.rs 内容进行分析，可能需要读取 Cargo.toml
3. 综合分析并回答

---

## 执行命令示例

```bash
# 场景 1: 单个工具调用
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test1 "请读取当前目录下的 Cargo.toml 文件的内容，并告诉我项目的名称。" --tools tools

# 场景 2: 多个工具调用
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test2 "请同时帮我做以下事情：1. 读取 src/lib.rs 文件的内容 2. 列出所有 .rs 文件。然后告诉我有多少个 Rust 源文件，以及 lib.rs 的前 50 个字符。" --tools tools

# 场景 3: 串行工具调用
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test3 "请列出所有 .md 文件，然后读取第一个 markdown 文件的内容，告诉我它的标题是什么。" --tools tools

# 场景 4: 无工具调用
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test4 "请用一句话解释什么是 Rust 编程语言。" --tools tools

# 场景 5: 错误处理
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test5 "请读取一个名为 nonexistent.txt 的文件，然后告诉我文件内容。" --tools tools

# 场景 6: 复杂多轮
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test6 "请分析当前项目的 Rust 源代码结构：1. 首先找出所有 .rs 文件 2. 读取 src/lib.rs 的内容 3. 检查是否使用了 async/await 4. 列出主要依赖的外部 crate。给我一个简洁的总结。" --tools tools

## OpenAI 兼容模式测试

# 场景 1: 单个工具调用 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test1 "请读取当前目录下的 Cargo.toml 文件的内容，并告诉我项目的名称。" --tools tools

# 场景 2: 多个工具调用 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test2 "请同时帮我做以下事情：1. 读取 src/lib.rs 文件的内容 2. 列出所有 .rs 文件。然后告诉我有多少个 Rust 源文件，以及 lib.rs 的前 50 个字符。" --tools tools

# 场景 3: 串行工具调用 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test3 "请列出所有 .md 文件，然后读取第一个 markdown 文件的内容，告诉我它的标题是什么。" --tools tools

# 场景 4: 无工具调用 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test4 "请用一句话解释什么是 Rust 编程语言。" --tools tools

# 场景 5: 错误处理 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test5 "请读取一个名为 nonexistent.txt 的文件，然后告诉我文件内容。" --tools tools

# 场景 6: 复杂多轮 (OpenAI)
cargo run --release --features cli --bin emx-llm -- chat -m openai.glm.glm-5 oai-test6 "请分析当前项目的 Rust 源代码结构：1. 首先找出所有 .rs 文件 2. 读取 src/lib.rs 的内容 3. 检查是否使用了 async/await 4. 列出主要依赖的外部 crate。给我一个简洁的总结。" --tools tools

# 启用原始输出模式查看工具结果
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test1 "请读取 Cargo.toml" --tools tools --raw

# 启用 token 统计
cargo run --release --features cli --bin emx-llm -- chat -m anthropic.glm.glm-5 test1 "请读取 Cargo.toml" --tools tools --token-stats
```

## 验证检查清单

### 1. Tool Call 发出正确性
- [ ] 工具名称正确
- [ ] 参数格式为有效 JSON
- [ ] 必需参数都包含
- [ ] 参数值符合预期

### 2. 多 Tool Call 处理
- [ ] 同一轮响应中包含多个 tool call
- [ ] 每个 tool call 有唯一的 id
- [ ] 工具执行顺序正确（并行 vs 串行）

### 3. 结果格式返回
- [ ] 工具结果正确添加到 session
- [ ] 工具结果格式符合预期（role: "tool"）
- [ ] tool_call_id 正确关联

### 4. 推理停止
- [ ] 无工具时直接返回文本
- [ ] 有工具时执行完工具调用后继续
- [ ] 最终响应后正确停止
- [ ] 不会无限循环

## 调试技巧

使用 `--raw` 模式可以看到完整的工具执行结果：

```bash
emx-llm chat -s test "prompt" --tools-dir tools --raw
```

使用 `--dry-run` 可以预览消息而不实际调用 API：

```bash
emx-llm chat -s test "prompt" --tools-dir tools --dry-run
```

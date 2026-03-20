# Session 设计：基于 mbox 的对话记录

## 1. 概述

使用 `emx-mbox` 作为 session 持久化后端，将 LLM 对话（system / user / assistant 消息）映射为 RFC 5322 邮件消息，存储在单个 `.mbox` 文件中。每个 session 对应一个 mbox 文件。

## 2. CLI 变更

### 2.1 当前 CLI

```
emx-llm chat -m <model> --prompt <file> "query text"
```

### 2.2 新 CLI

```
emx-llm chat [OPTIONS] <SESSION> [PROMPT]

Arguments:
  <SESSION>    Session 名称 (不带 .mbox 后缀)【必填】
  [PROMPT]     Prompt 文本，或以 @ 开头的文件路径 (如 @prompt.txt)

Options:
  -m, --model <MODEL>       模型名称
  -s, --system <SYSTEM>     System prompt 文本，或以 @ 开头的文件路径 (仅新 session 有效)
      --api-base <URL>      API base URL (覆盖配置)
      --stream              启用流式输出 (默认)
      --no-stream           禁用流式输出
      --dry-run             仅输出构建的 messages，不发送请求
      --token-stats         显示 token 用量统计
      --attach <FILE>...    附加文件作为上下文 (可多次指定)

Environment Variables:
  EMX_SESSION_DIR           Session 存储目录 (默认 ~/.local/share/emx-llm/sessions/)
  EMX_DOMAIN                邮件地址域名 (默认 emx-llm)
```

### 2.3 使用示例

```bash
# 新建 session 进行对话
emx-llm chat my-session --model gpt-4 "Hello"

# 新建 session 并指定 system prompt
emx-llm chat my-session --system "You are a Rust expert" --model gpt-4 "Explain lifetimes"

# system prompt 从文件加载
emx-llm chat my-session --system @rust-expert.txt -m gpt-4 "Explain lifetimes"

# 继续已有 session（多轮对话）
emx-llm chat my-session "Now add error handling"

# 继续已有 session + 附加文件
emx-llm chat my-session "Review this" --attach src/main.rs

# Prompt 从文件加载
emx-llm chat my-session @question.txt

# 从 stdin 读取 prompt (无 PROMPT 参数时)
echo "Hello" | emx-llm chat -m gpt-4 my-session
```

### 2.4 参数解析规则

- `SESSION` 为必填参数
- `PROMPT` 为可选参数：
  - 若提供 → 作为 prompt 文本（或以 `@` 开头的文件路径）
  - 若省略 → 从 stdin 读取 prompt

## 3. Mbox 中的消息映射

### 3.1 核心思路

利用邮件的 **From (发件人)** 字段区分消息角色，域名可通过 `EMX_DOMAIN` 环境变量配置（默认 `emx-llm`）：

| MessageRole | From 字段 | 说明 |
|---|---|---|
| `System` | `system@<domain>` | System prompt |
| `User` | `user@<domain>` | 用户输入 |
| `Assistant` | `<model>@<domain>` | LLM 回复 (使用实际模型名，如 `gpt-4@emx-llm`) |
| `Agent` | `<agent>#<model>@<domain>` | Agent 模式调用 (如 `coder#gpt-4@emx-llm`) |
| `Tool` | `tool@<domain>` | 工具调用结果 (参考 OpenAI/Anthropic API 规范) |

### 3.2 消息结构

一条 mbox 消息的完整格式：

```
From user@emx-llm Wed Mar 19 10:30:00 2026
From: user@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:30:00 +0800
Message-ID: <uuid@emx-llm>
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
X-LLM-Tokens: prompt=150; completion=0; total=150

Hello, explain Rust lifetimes to me.
```

Assistant 消息示例（使用模型名作为发件人）：

```
From gpt-4@emx-llm Wed Mar 19 10:30:05 2026
From: gpt-4@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:30:05 +0800
Message-ID: <uuid@emx-llm>
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
X-LLM-Tokens: prompt=45; completion=320; total=365
X-LLM-Duration-Ms: 3200

Lifetimes in Rust are a way of expressing ...
```

Agent 模式消息示例（agent 调用 model）：

```
From coder#gpt-4@emx-llm Wed Mar 19 10:31:00 2026
From: coder#gpt-4@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:31:00 +0800
Message-ID: <uuid@emx-llm>
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
X-LLM-Tokens: prompt=100; completion=200; total=300
X-LLM-Duration-Ms: 2800

I'll help you write that function...
```

### 3.3 自定义 Header

| Header | 适用角色 | 说明 |
|---|---|---|
| `X-LLM-Tokens` | assistant/agent | Token 用量，格式：`prompt=N; completion=N; total=N` |
| `X-LLM-Duration-Ms` | assistant/agent | 请求耗时 (毫秒) |

**说明：**
- **角色识别**：通过 `From` 字段推断，无需额外的 Role header
  - `system@...` → System
  - `user@...` → User
  - `tool@...` → Tool
  - `<agent>#<model>@...` → Agent (包含 `#`)
  - `<model>@...` → Assistant (不含 `#`)
- **模型/Agent 名称**：通过 `From` 字段直接获取，无需单独 header
- **Cost 估算**：不记录（可从 token 数计算）
- **Stream 标记**：不记录（与存储无关）

### 3.4 Subject 处理

- 当前实现中 Subject 可为空字符串
- 后续可在 session 结束时，通过小模型对对话内容进行摘要，回填到第一条消息的 Subject 中
- 也可由用户通过命令手动设置

### 3.5 附件处理

当指定 `--attach <FILE>` 时：
- 文件内容作为 user 消息的一部分，使用 MIME multipart 附加
- 利用 `emx-mbox` 现有的 `MessageBuilder::attach_file()` 支持
- 在构造 LLM 请求时，将附件内容提取并拼接到 user message content 中

## 4. Session 文件布局

Session 目录通过 `EMX_SESSION_DIR` 环境变量配置，默认为 `~/.local/share/emx-llm/sessions/`。

```
$EMX_SESSION_DIR/  (默认 ~/.local/share/emx-llm/sessions/)
├── my-project.mbox            # 用户命名的 session
└── code-review.mbox           # 另一个命名 session
```

- 文件名即 session 标识，不含 `.mbox` 后缀
- 新 session 首次写入时创建文件
- 续接 session 使用 `MboxWriter::open_append()` 追加

## 5. Session 生命周期

### 5.1 新建 Session

```
[start]
  │
  ▼
┌─────────────────────────────────┐
│ 1. 解析 CLI 参数                 │
│ 2. 确定 session name            │
│ 3. 检查 .mbox 文件是否已存在     │
│    ├── 不存在 → 新建             │
│    └── 存在   → 加载历史消息      │
└─────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────┐
│ 4. System Prompt 一致性检查:    │
│    ├── 新 session:              │
│    │   若有 --system，写入指定的 │
│    │   若无 --system，写入默认的 │
│    │   (默认 prompt 从 embed 的 │
│    │    src/prompts/system.md)  │
│    └── 已有 session:            │
│        若有 --system 且与历史    │
│        中的 system prompt 不一致 │
│        → 报错退出，拒绝推理      │
└─────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────┐
│ 5. 构建 messages:               │
│    a. 从 mbox 加载历史 messages  │
│    b. 写入当前 user message      │
│    c. 追加写入 mbox 文件          │
└─────────────────────────────────┘
  │
  ▼
┌─────────────────────────────────┐
│ 6. 发送 API 请求 (stream/sync)  │
│ 7. 收到完整响应后:               │
│    a. 写入 assistant message    │
│       (含 token stats header)   │
│    b. 追加写入 mbox 文件         │
└─────────────────────────────────┘
  │
  ▼
[end]
```

### 5.2 继续已有 Session

- 从 mbox 文件读取全部历史消息
- 若指定了 `--system`，需与历史中的 system prompt 比较：
  - **一致**：正常继续
  - **不一致**：报错退出，提示 system prompt 冲突
- 将历史消息 + 新 user message 一起发送给 LLM
- 仅追加新 user message 和 assistant response 到 mbox 文件

## 6. 模块设计

### 6.1 新增模块 `src/session.rs`

```rust
/// Session 管理模块 — 基于 emx-mbox 的对话持久化
use emx_mbox::{Mbox, MboxWriter, MessageBuilder, MailMessage};

/// 默认 system prompt (embed 的 src/prompts/system.md)
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/system.md");

/// 邮件地址域名 (通过 EMX_DOMAIN 环境变量配置，默认 emx-llm)
fn get_domain() -> String {
    std::env::var("EMX_DOMAIN").unwrap_or_else(|_| "emx-llm".to_string())
}

/// 消息角色对应的邮件地址前缀
const SYSTEM_PREFIX: &str = "system";
const USER_PREFIX: &str = "user";
const TOOL_PREFIX: &str = "tool";
// Assistant 使用模型名作为前缀
// Agent 使用 <agent>#<model> 格式

/// 从 MailMessage 的 From 字段推断 MessageRole
/// - system@... → System
/// - user@... → User
/// - tool@... → Tool
/// - <agent>#<model>@... → Agent (包含 #)
/// - <model>@... → Assistant (不含 #)
pub fn role_from_mail(msg: &MailMessage) -> MessageRole { ... }

/// 从 MailMessage 的 From 字段提取模型名称 (Assistant/Agent 消息)
/// Assistant: 返回模型名
/// Agent: 返回 (agent_name, model_name)
pub fn parse_from_address(msg: &MailMessage) -> FromInfo { ... }

/// Session 句柄
pub struct Session {
    /// Session 名称
    name: String,
    /// mbox 文件路径
    path: PathBuf,
    /// 已加载的历史消息
    history: Vec<Message>,
    /// 历史 system prompt (若存在)
    system_prompt: Option<String>,
}

impl Session {
    /// 打开或创建 session
    /// session_dir 通过 EMX_SESSION_DIR 环境变量获取
    pub fn open(name: &str) -> Result<Self>;

    /// 获取 session 目录 (从环境变量)
    fn get_session_dir() -> PathBuf;

    /// 从 mbox 加载历史并转为 Message 列表
    fn load_history(path: &Path) -> Result<Vec<Message>>;

    /// 验证 system prompt 一致性
    /// 若 session 已存在且 system prompt 不一致，返回错误
    pub fn validate_system_prompt(&self, provided: Option<&str>) -> Result<()>;

    /// 追加一条消息到 mbox 文件
    pub fn append(&self, msg: &Message, model: Option<&str>, usage: Option<&Usage>) -> Result<()>;

    /// 获取完整 message 列表 (历史 + 待发送)
    pub fn messages(&self) -> &[Message];

    /// 追加 user message 并返回更新后的 message 列表
    pub fn add_user_message(&mut self, content: String, attachments: &[PathBuf]) -> Result<&[Message]>;

    /// 追加 assistant response (含 token 统计)
    pub fn add_assistant_response(&mut self, content: String, model: &str, usage: &Usage) -> Result<()>;
}
```

### 6.2 修改 `src/bin/emx-llm/cli.rs`

将 Chat 子命令的参数调整为新格式（参见 §2.2）。

### 6.3 修改 `src/bin/emx-llm/chat.rs`

集成 `Session` 模块：
1. 从 CLI 参数获取 session name（必填）
2. 打开/创建 session
3. 处理 system prompt 和 user prompt
4. 发送请求、接收响应
5. 将响应追加到 session

## 7. 依赖变更

### 7.1 Embedded Resources

默认 system prompt 通过 `include_str!` embed 到二进制文件中：

```
src/prompts/
└── system.md        # 默认 system prompt
```

在代码中引用：
```rust
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/system.md");
```

### 7.2 Cargo.toml

```toml
[dependencies]
# 新增: session 记录
emx-mbox = { path = "../emx-mbox" }      # 开发时使用 path
# emx-mbox = { git = "..." }             # 发布时切换为 git
```

`emx-mbox` 的依赖 (`chrono`, `uuid`) 已在 emx-llm 的 `cli` feature 中，无额外引入。

## 8. 多轮对话示例

### 8.1 mbox 文件内容示例

```
From system@emx-llm Wed Mar 19 10:30:00 2026
From: system@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:30:00 +0800
Message-ID: <550e8400-e29b-41d4-a716-446655440000@emx-llm>
Content-Type: text/plain; charset=utf-8

You are a Rust expert. Explain concepts clearly with examples.

From user@emx-llm Wed Mar 19 10:30:01 2026
From: user@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:30:01 +0800
Message-ID: <550e8400-e29b-41d4-a716-446655440001@emx-llm>
Content-Type: text/plain; charset=utf-8

Explain Rust lifetimes to me.

From gpt-4@emx-llm Wed Mar 19 10:30:05 2026
From: gpt-4@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:30:05 +0800
Message-ID: <550e8400-e29b-41d4-a716-446655440002@emx-llm>
Content-Type: text/plain; charset=utf-8
X-LLM-Tokens: prompt=45; completion=320; total=365
X-LLM-Duration-Ms: 3200

Lifetimes in Rust are a way of expressing ...

From user@emx-llm Wed Mar 19 10:31:00 2026
From: user@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:31:00 +0800
Message-ID: <550e8400-e29b-41d4-a716-446655440003@emx-llm>
Content-Type: text/plain; charset=utf-8

Can you show me an example with structs?

From gpt-4@emx-llm Wed Mar 19 10:31:08 2026
From: gpt-4@emx-llm
Subject:
Date: Wed, 19 Mar 2026 10:31:08 +0800
Message-ID: <550e8400-e29b-41d4-a716-446655440004@emx-llm>
Content-Type: text/plain; charset=utf-8
X-LLM-Tokens: prompt=380; completion=450; total=830
X-LLM-Duration-Ms: 4100

Sure! Here's a struct with lifetime annotations ...
```

## 9. 向后兼容

- 原有 `--prompt` 参数更名为 `--system`（语义更清晰）
- 原有位置参数 `query` 拆分为必填的 `SESSION` 和可选的 `PROMPT`
- 用户需要显式指定 session 名称
- stdin 输入仍然支持（无 PROMPT 参数时）

## 10. 未来扩展

- [ ] `emx-llm session list` — 列出所有 session
- [ ] `emx-llm session show <name>` — 查看 session 历史
- [ ] `emx-llm session delete <name>` — 删除 session
- [ ] `emx-llm session summarize <name>` — 用小模型生成 Subject 摘要
- [ ] `emx-llm session export <name> --format md|json` — 导出对话
- [ ] session 大小限制 / token 窗口裁剪策略
- [ ] `@` 前缀约定同时适用于 `--system` 和 `PROMPT`（从文件加载内容）
- [ ] 工具调用支持 — 完善 Tool 角色消息格式，参考 OpenAI/Anthropic API 规范：
  - Tool 消息需包含 `tool_call_id` (OpenAI) 或对应标识
  - 支持 `X-LLM-Tool-Call-Id` header 存储工具调用标识
  - 支持 Assistant 消息中的工具调用请求存储

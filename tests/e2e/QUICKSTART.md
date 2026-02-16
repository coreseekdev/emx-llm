# emx-gate E2E 测试快速开始

## 快速开始

### 1. 添加测试依赖

已在 `Cargo.toml` 中配置：
```toml
[dev-dependencies]
emx-testspec = { git = "https://github.com/coreseekdev/emx-testspec" }
```

### 2. 编译测试

```bash
# 编译 emx-gate（带 gate feature）
cargo build --bin emx-gate --features gate

# 确保二进制文件可用
./target/debug/emx-gate.exe --help
```

### 3. 运行单个测试

```bash
# 使用 emx-testspec CLI 运行
emx-testspec tests/e2e/001-health-check.txtar

# 或使用 cargo test
cargo test --test e2e_health_check
```

### 4. 运行所有 E2E 测试

```bash
# 使用 cargo test
cargo test --test e2e

# 详细输出
E2E_VERBOSE=1 cargo test --test e2e

# 保留工作目录（调试用）
emx-testspec tests/e2e/ --keep
```

## 测试文件说明

| 文件 | 测试内容 | 验证点 |
|------|---------|--------|
| `001-health-check.txtar` | 健康检查端点 | 状态码、响应结构 |
| `002-openai-chat.txtar` | OpenAI 聊天端点 | 请求格式、响应结构 |
| `003-anthropic-messages.txtar` | Anthropic 消息端点 | API 兼容性 |
| `004-list-endpoints.txtar` | 模型/Provider 列表 | 数据完整性 |
| `005-error-handling.txtar` | 错误处理 | HTTP 状态码 |

## 编写新测试

### 模板

```txtar
# Test description

# Start gateway
exec emx-gate &
sleep 2s

# Your test here
exec curl -s http://127.0.0.1:8848/your-endpoint
stdout 'expected output'

# Clean up
[unix] exec pkill -f emx-gate
[windows] exec taskkill //F //IM emx-gate.exe
```

### 常用命令

| 命令 | 说明 | 示例 |
|------|------|------|
| `exec` | 执行命令 | `exec curl -s http://...` |
| `stdout` | 匹配 stdout（正则） | `stdout '"status":"ok"'` |
| `stderr` | 匹配 stderr | `stderr 'error'` |
| `!` | 命令必须失败 | `! exec invalid-command` |
| `?` | 命令可成功可失败 | `? exec flaky-command` |
| `sleep` | 等待 | `sleep 2s` |
| `env` | 设置环境变量 | `env KEY=value` |
| `[unix]` | Unix 条件执行 | `[unix] exec unix-tool` |
| `[windows]` | Windows 条件执行 | `[windows] exec windows-tool` |

## 故障排查

### 测试失败

```bash
# 保留工作目录
emx-testspec tests/e2e/ --keep

# 查看工作目录
ls -la /tmp/emx-testspec-*

# 进入工作目录
cd /tmp/emx-testspec-xxx

# 手动执行脚本
cat script.txt
bash script.txt
```

### 端口被占用

```bash
# 检查端口占用
netstat -ano | grep 8848  # Windows
lsof -i :8848            # Unix/macOS

# 杀掉占用进程
taskkill //F //PID xxx   # Windows
kill -9 xxx             # Unix
```

### 服务器未启动

```bash
# 查看日志
cat $WORK/*.log

# 检查 emx-gate 是否可用
which emx-gate
./target/debug/emx-gate.exe --help
```

## 系统要求

- **工具**: `curl`（HTTP 客户端）
- **权限**: 能绑定 8848 端口
- **平台**: Windows/macOS/Linux

## 集成到 CI

```yaml
# .github/workflows/e2e.yml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]

    steps:
      - uses: actions/checkout@v3
      - uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Build emx-gate
        run: cargo build --bin emx-gate --features gate

      - name: Run E2E tests
        run: cargo test --test e2e
```

## 相关文档

- [emx-testspec README](https://github.com/coreseekdev/emx-testspec)
- [测试设计文档](./README.md)

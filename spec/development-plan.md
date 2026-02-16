# 开发计划和步骤

## 开发阶段划分

### Phase 0: 准备阶段（1-2天）
**目标**：搭建开发环境和基础设施

#### 步骤 0.1: 依赖添加
```toml
# Cargo.toml
[dependencies]
axum = "0.7"
tower = "0.4"
tower-http = "0.5"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

#### 步骤 0.2: 目录结构创建
```
src/gate/
├── mod.rs
├── server.rs
├── router.rs
├── handlers.rs
└── config.rs
```

#### 步骤 0.3: 基础配置结构
```rust
// src/gate/config.rs
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
}

// 扩展现有 LlmConfig
pub struct LlmConfig {
    // 现有字段...

    // Gateway 配置
    #[serde(default)]
    pub gate: Option<GatewayConfig>,
}
```

#### 步骤 0.4: CLI 子命令添加
```rust
// src/bin/emx-llm.rs
#[derive(Subcommand)]
enum Commands {
    Chat(ChatArgs),
    Test(TestArgs),
    Gate(GateArgs),  // 新增
}
```

**验证**：
- `emx-llm gate --help` 显示帮助信息
- `emx-llm gate --validate` 验证配置

---

### Phase 1: 核心 HTTP 服务器（2-3天）
**目标**：搭建基础 HTTP 服务器框架

#### 步骤 1.1: 基础 HTTP 服务器
```rust
// src/gate/server.rs
pub async fn start_server(config: GatewayConfig) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

#### 步骤 1.2: 健康检查端点
```rust
async fn health_check() -> Json<Value> {
    json!({
        "status": "ok",
        "timestamp": Utc::now().to_rfc3339()
    })
}
```

#### 步骤 1.3: 模型列表端点
```rust
async fn list_models(State(state): State<GatewayState>) -> Json<Value> {
    let models = state.config.list_models();
    json!({ "data": models })
}
```

#### 步骤 1.4: 请求日志中间件
```rust
async fn logging_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let duration = start.elapsed();
    println!("[{}] {} {} - {:?}",
        response.status(), method, uri, duration);

    Ok(response)
}
```

**验证**：
```bash
# 启动服务器
emx-llm gate

# 测试健康检查
curl http://localhost:8080/health

# 测试模型列表
curl http://localhost:8080/v1/models
```

---

### Phase 2: 路由解析（2-3天）
**目标**：实现模型名称解析和配置查找

#### 步骤 2.1: 模型引用解析
```rust
// src/gate/router.rs
pub struct ModelReference {
    pub provider_type: ProviderType,
    pub config_path: Vec<String>,
    pub model_name: String,
}

pub fn parse_model_reference(model: &str) -> Result<ModelReference> {
    let parts: Vec<&str> = model.split('.').collect();

    match parts.len() {
        1 => {
            // 短名称：在所有配置中查找
            find_model_by_short_name(model)
        }
        2 => {
            // 限定名称：provider.model
            let provider_type = ProviderType::from_str(parts[0])?;
            Ok(ModelReference {
                provider_type,
                config_path: vec![],
                model_name: parts[1].to_string(),
            })
        }
        _ => {
            // 完全限定名称：provider.sub1.sub2.model
            let provider_type = ProviderType::from_str(parts[0])?;
            Ok(ModelReference {
                provider_type,
                config_path: parts[1..parts.len()-1].iter()
                    .map(|s| s.to_string())
                    .collect(),
                model_name: parts.last().unwrap().to_string(),
            })
        }
    }
}
```

#### 步骤 2.2: 配置查找
```rust
pub fn resolve_provider_config(
    config: &LlmConfig,
    model_ref: &ModelReference,
) -> Result<ProviderConfig> {
    // 构建配置路径
    let base_path = format!("llm.provider.{}", model_ref.provider_type);
    let full_path = if model_ref.config_path.is_empty() {
        base_path
    } else {
        format!("{}.{}", base_path, model_ref.config_path.join("."))
    };

    // 查找配置（从具体到一般）
    config.get_provider_config(&full_path)
        .or_else(|| config.get_provider_config(&base_path))
        .ok_or_else(|| Error::ProviderNotFound(full_path))
}
```

#### 步骤 2.3: 客户端创建
```rust
pub fn create_client_for_model(
    config: &LlmConfig,
    model: &str,
) -> Result<Box<dyn Client>> {
    let model_ref = parse_model_reference(model)?;
    let provider_config = resolve_provider_config(config, &model_ref)?;
    create_client(provider_config)
}
```

**验证**：
```bash
# 测试短名称解析
curl http://localhost:8080/v1/models | jq '.data[] | select(.id == "gpt-4")'

# 测试限定名称
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -d '{"messages":[{"role":"user","content":"test"}]}'

# 测试完全限定名称
curl -X POST http://localhost:8080/anthropic.glm.glm-5/messages \
  -d '{"messages":[{"role":"user","content":"test"}]}'
```

---

### Phase 3: Provider 转发（3-4天）
**目标**：实现 OpenAI 和 Anthropic 请求转发

#### 步骤 3.1: 路由注册
```rust
// src/gate/handlers.rs
pub fn create_routes() -> Router {
    Router::new()
        .route("/openai/:model/chat/completions", post(openai_handler))
        .route("/anthropic/:model/messages", post(anthropic_handler))
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .layer(logging_middleware())
}
```

#### 步骤 3.2: OpenAI 处理器
```rust
async fn openai_handler(
    State(state): State<GatewayState>,
    Path(model): Path<String>,
    Json(request): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // 创建客户端
    let client = create_client_for_model(&state.config, &format!("openai.{}", model))
        .map_err(|e| {
            error!("Failed to create client: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 解析消息
    let messages: Vec<Message> = serde_json::from_value(
        request["messages"].clone()
    ).map_err(|_| StatusCode::BAD_REQUEST)?;

    // 调用 provider
    let (response, usage) = client.chat(&messages, &model).await
        .map_err(|e| {
            error!("Provider error: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    // 返回响应
    Ok(Json(json!({
        "id": "chatcmpl-xxx",
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response
            },
            "finish_reason": "stop"
        }],
        "usage": usage
    })))
}
```

#### 步骤 3.3: Anthropic 处理器
```rust
async fn anthropic_handler(
    State(state): State<GatewayState>,
    Path(model): Path<String>,
    Json(request): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // 类似 OpenAI 处理器
    // 使用 Anthropic API 格式
}
```

#### 步骤 3.4: 错误处理
```rust
pub fn error_response(status: StatusCode, message: &str) -> Json<Value> {
    Json(json!({
        "error": {
            "message": message,
            "type": "api_error",
            "code": status.as_u16()
        }
    }))
}
```

**验证**：
```bash
# OpenAI 请求
curl -X POST http://localhost:8080/openai/gpt-4/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Anthropic 请求
curl -X POST http://localhost:8080/anthropic/claude-3-opus-20240229/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-opus-20240229",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

---

### Phase 4: 集成和测试（2-3天）
**目标**：完成集成、测试和文档

#### 步骤 4.1: 配置验证
```rust
pub fn validate_gateway_config(config: &LlmConfig) -> Result<()> {
    if let Some(gate_config) = &config.gate {
        // 验证端口范围
        if gate_config.port < 1024 || gate_config.port > 65535 {
            return Err(Error::InvalidPort(gate_config.port));
        }

        // 验证至少有一个 provider
        if config.provider.is_none() {
            return Err(Error::NoProviderConfigured);
        }
    }
    Ok(())
}
```

#### 步骤 4.2: 集成测试
```rust
#[tokio::test]
async fn test_gateway_openai_request() {
    let config = load_test_config();
    let server = start_test_server(config).await;

    let response = reqwest::Client::new()
        .post(format!("{}/openai/gpt-4/chat/completions", server.addr()))
        .json(&json!({"messages": [{"role": "user", "content": "test"}]}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}
```

#### 步骤 4.3: 文档编写
- README 更新
- API 文档
- 配置示例
- 使用指南

#### 步骤 4.4: 发布准备
- 版本号更新
- CHANGELOG
- Release Notes

**验证**：
```bash
# 完整测试流程
emx-llm gate --test

# 端到端测试
./tests/integration/test_gateway.sh
```

---

## 开发时间线

| 阶段 | 工作量 | 里程碑 |
|-----|--------|--------|
| Phase 0: 准备 | 1-2天 | CLI 命令可用，配置结构就绪 |
| Phase 1: HTTP 服务器 | 2-3天 | 可以启动服务器，健康检查正常 |
| Phase 2: 路由解析 | 2-3天 | 模型名称解析正确 |
| Phase 3: Provider 转发 | 3-4天 | OpenAI 和 Anthropic 请求成功 |
| Phase 4: 集成测试 | 2-3天 | 测试通过，文档完整 |
| **总计** | **10-15天** | MVP 可发布 |

## 并行开发建议

### 可并行进行的任务
1. Phase 0.3（配置结构）和 Phase 0.4（CLI）可并行
2. Phase 1.2（健康检查）和 Phase 1.3（模型列表）可并行
3. Phase 3.2（OpenAI）和 Phase 3.3（Anthropic）可并行

### 依赖关系
```
Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 4
           ↑__________|
           (可并行)
```

## 测试策略

### 单元测试
每个模块独立测试：
- 路由解析逻辑
- 配置查找
- 错误处理

### 集成测试
端到端测试流程：
- 启动服务器
- 发送请求
- 验证响应

### 手动测试
使用真实 API 测试：
- OpenAI GPT-4
- Anthropic Claude
- 第三方兼容服务（GLM）

## 风险管理

### 已知风险
1. **Provider API 变化**：使用稳定版本，做好错误处理
2. **性能问题**：使用异步，添加超时控制
3. **配置兼容性**：充分测试，向后兼容

### 缓解措施
1. 充分的单元测试和集成测试
2. 详细的错误日志
3. 配置验证
4. 渐进式发布

## 发布检查清单

- [ ] 所有功能测试通过
- [ ] 文档完整
- [ ] 配置示例可运行
- [ ] 集成测试通过
- [ ] 性能测试通过
- [ ] 错误处理完善
- [ ] 日志输出清晰
- [ ] 代码审查通过
- [ ] 版本号更新
- [ ] CHANGELOG 更新

达到 MVP 标准，可以发布 v0.1.0 版本。

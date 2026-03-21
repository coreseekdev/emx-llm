//! Chat command implementation

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};
use emx_llm::{create_client, create_client_for_model, load_with_default, load_tools_from_dir, ProviderConfig, Session, Usage, ToolCall};
use futures::StreamExt;

/// Run the chat command
#[allow(clippy::too_many_arguments)]
pub async fn run(
    session: String,
    prompt: Option<String>,
    model: Option<String>,
    api_base: Option<String>,
    stream: bool,
    no_stream: bool,
    system: Option<String>,
    dry_run: bool,
    token_stats: bool,
    attach: Vec<PathBuf>,
    tools_dir: Option<PathBuf>,
    raw: bool,
) -> Result<()> {
    run_async(
        session,
        prompt,
        model,
        api_base,
        stream,
        no_stream,
        system,
        dry_run,
        token_stats,
        attach,
        tools_dir,
        raw,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_async(
    session_name: String,
    prompt: Option<String>,
    model: Option<String>,
    api_base: Option<String>,
    stream: bool,
    no_stream: bool,
    system: Option<String>,
    dry_run: bool,
    token_stats: bool,
    attach: Vec<PathBuf>,
    tools_dir: Option<PathBuf>,
    raw: bool,
) -> Result<()> {
    let (client, model_id) = resolve_client(model.as_deref(), api_base.as_deref())?;

    let mut session = Session::open(&session_name)?;
    let system_prompt = match system {
        Some(value) => Some(resolve_input_value(&value)?),
        None => None,
    };

    session.ensure_system_prompt(system_prompt.as_deref())?;

    let prompt_text = resolve_prompt(prompt)?;
    if prompt_text.trim().is_empty() {
        return Err(anyhow!("prompt is empty; provide PROMPT or stdin content"));
    }

    if dry_run {
        let messages = session.preview_user_message(prompt_text, &attach)?;
        println!("=== Dry Run Mode ====");
        println!("Session: {}", session.name());
        println!("Session File: {}", session.path().display());
        println!("API Base: {}", client.api_base());
        println!("Model: {}", model_id);
        println!("Max Tokens: {}", client.max_tokens());
        println!();

        println!("Messages:");
        for msg in &messages {
            match msg.role {
                emx_llm::MessageRole::System => println!("  [System]: {}", msg.get_content().unwrap_or("")),
                emx_llm::MessageRole::User => println!("  [User]: {}", msg.get_content().unwrap_or("")),
                emx_llm::MessageRole::Assistant => println!("  [Assistant]: {}", msg.get_content().unwrap_or("")),
                emx_llm::MessageRole::Tool => println!("  [Tool]: {}", msg.get_content().unwrap_or("")),
            }
        }
        println!();
        println!("Total: {} messages", messages.len());
        return Ok(());
    }

    session.add_user_message(prompt_text, &attach)?;

    // Load tools from tools directory
    let tools = load_tools_from_dir(tools_dir.as_deref())?;

    let messages = session.messages().to_vec();
    let use_stream = stream || !no_stream;

    if use_stream {
        let started = Instant::now();
        let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
        let mut response_stream = client.chat_stream(&messages, &model_id, tools_ref);
        let mut full_response = String::new();
        let mut final_usage: Option<Usage> = None;
        let mut final_tool_calls: Option<Vec<ToolCall>> = None;

        while let Some(event) = response_stream.next().await {
            match event {
                Ok(event) => {
                    print!("{}", event.delta);
                    io::stdout().flush()?;

                    full_response.push_str(&event.delta);

                    if event.done {
                        final_usage = event.usage;
                        final_tool_calls = event.tool_calls;
                    }
                }
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    break;
                }
            }
        }

        let usage = final_usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        // Handle tool calls from streaming response
        if let Some(calls) = final_tool_calls {
            println!("\n[Tool Calls: {}]", calls.len());
            for (i, call) in calls.iter().enumerate() {
                println!("  [{}] {}: {}", i + 1, call.name, call.arguments);
            }

            // Save assistant tool calls to session
            session.add_assistant_tool_calls(
                calls.clone(),
                &model_id,
                &usage,
                Some(started.elapsed().as_millis()),
            )?;

            // Execute each tool call
            for call in &calls {
                let result = execute_tool_call(call, tools_dir.as_ref())?;
                if raw {
                    println!("\n[Tool Result: {}]\n{}", call.name, result);
                } else {
                    println!("[Executed: {}]", call.name);
                }
                session.add_tool_result(call.id.clone(), result)?;
            }

            // Continue conversation with tool results
            let follow_up_messages = session.messages().to_vec();
            let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
            let mut follow_up_stream = client.chat_stream(&follow_up_messages, &model_id, tools_ref);
            let mut follow_up_response = String::new();
            let mut follow_up_usage: Option<Usage> = None;

            while let Some(event) = follow_up_stream.next().await {
                match event {
                    Ok(event) => {
                        print!("{}", event.delta);
                        io::stdout().flush()?;
                        follow_up_response.push_str(&event.delta);
                        if event.done {
                            follow_up_usage = event.usage;
                        }
                    }
                    Err(e) => {
                        eprintln!("Stream error: {}", e);
                        break;
                    }
                }
            }

            if !follow_up_response.is_empty() {
                let fu_usage = follow_up_usage.unwrap_or(Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                });
                session.add_assistant_response(
                    follow_up_response,
                    &model_id,
                    &fu_usage,
                    Some(started.elapsed().as_millis()),
                )?;

                if token_stats {
                    println!();
                    println!("=== Token Stats ===");
                    println!("Prompt tokens: {}", usage.prompt_tokens + fu_usage.prompt_tokens);
                    println!("Completion tokens: {}", usage.completion_tokens + fu_usage.completion_tokens);
                    println!("Total tokens: {}", usage.total_tokens + fu_usage.total_tokens);
                    println!("Duration (ms): {}", started.elapsed().as_millis());
                }
            }
        } else if !full_response.is_empty() {
            session.add_assistant_response(
                full_response,
                &model_id,
                &usage,
                Some(started.elapsed().as_millis()),
            )?;

            if token_stats {
                println!();
                println!("=== Token Stats ===");
                println!("Prompt tokens: {}", usage.prompt_tokens);
                println!("Completion tokens: {}", usage.completion_tokens);
                println!("Total tokens: {}", usage.total_tokens);
                println!("Duration (ms): {}", started.elapsed().as_millis());
            }
        }
    } else {
        // Non-streaming mode with tool call loop
        let started = Instant::now();
        let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
        let (response, tool_calls, usage) = client.chat(&messages, &model_id, tools_ref).await?;

        // Handle tool calls if present
        if let Some(calls) = tool_calls {
            println!("[Tool Calls: {}]", calls.len());
            for (i, call) in calls.iter().enumerate() {
                println!("  [{}] {}: {}", i + 1, call.name, call.arguments);
            }

            // Add assistant tool calls to session
            session.add_assistant_tool_calls(
                calls.clone(),
                &model_id,
                &usage,
                Some(started.elapsed().as_millis()),
            )?;

            // Execute each tool call
            for call in &calls {
                let result = execute_tool_call(call, tools_dir.as_ref())?;

                // Show tool result (if --raw, show raw output)
                if raw {
                    println!("\n[Tool Result: {}]\n{}", call.name, result);
                } else {
                    println!("[Executed: {}]", call.name);
                }

                // Add tool result to session
                session.add_tool_result(call.id.clone(), result)?;
            }

            // Continue the conversation with tool results
            let follow_up_messages = session.messages().to_vec();
            let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
            let (follow_up, _, follow_up_usage) = client.chat(&follow_up_messages, &model_id, tools_ref).await?;
            println!("{}", follow_up);

            session.add_assistant_response(
                follow_up,
                &model_id,
                &follow_up_usage,
                Some(started.elapsed().as_millis()),
            )?;

            if token_stats {
                println!();
                println!("=== Token Stats ===");
                println!("Prompt tokens: {}", usage.prompt_tokens + follow_up_usage.prompt_tokens);
                println!("Completion tokens: {}", usage.completion_tokens + follow_up_usage.completion_tokens);
                println!("Total tokens: {}", usage.total_tokens + follow_up_usage.total_tokens);
                println!("Duration (ms): {}", started.elapsed().as_millis());
            }
        } else {
            // No tool calls, normal response
            println!("{}", response);

            session.add_assistant_response(
                response,
                &model_id,
                &usage,
                Some(started.elapsed().as_millis()),
            )?;

            if token_stats {
                println!();
                println!("=== Token Stats ===");
                println!("Prompt tokens: {}", usage.prompt_tokens);
                println!("Completion tokens: {}", usage.completion_tokens);
                println!("Total tokens: {}", usage.total_tokens);
                println!("Duration (ms): {}", started.elapsed().as_millis());
            }
        }
    }

    Ok(())
}

fn resolve_client(
    model_ref: Option<&str>,
    api_base_override: Option<&str>,
) -> Result<(Box<dyn emx_llm::Client>, String)> {
    if let Some(model_ref) = model_ref {
        if let Some(api_base) = api_base_override {
            let (model_config, model_id) = ProviderConfig::load_for_model(model_ref)?;
            let client = create_client(ProviderConfig {
                provider_type: model_config.provider_type,
                api_base: api_base.to_string(),
                api_key: model_config.api_key,
                model: Some(model_id.clone()),
                max_tokens: model_config.max_tokens,
                timeout_secs: None,
            })?;
            return Ok((client, model_id));
        }
        return create_client_for_model(model_ref);
    }

    let mut config = load_with_default()?;
    if let Some(api_base) = api_base_override {
        config.api_base = api_base.to_string();
    }

    let model_id = config
        .model
        .as_ref()
        .ok_or_else(|| anyhow!("No model configured. Set llm.provider.model"))?
        .clone();

    let client = create_client(config)?;
    Ok((client, model_id))
}

fn resolve_prompt(prompt: Option<String>) -> Result<String> {
    match prompt {
        Some(value) => resolve_input_value(&value),
        None => {
            let stdin = io::stdin();
            let mut buffer = String::new();
            stdin.lock().read_to_string(&mut buffer)?;
            Ok(buffer.trim().to_string())
        }
    }
}

fn resolve_input_value(value: &str) -> Result<String> {
    if let Some(path) = value.strip_prefix('@') {
        return Ok(std::fs::read_to_string(path)?);
    }
    Ok(value.to_string())
}

/// Execute tool calls by calling TCL scripts
fn execute_tool_call(tool_call: &ToolCall, tools_dir: Option<&PathBuf>) -> Result<String> {
    let tools_dir_path = tools_dir.as_ref().map(|p| p.as_path()).unwrap_or_else(|| {
        std::path::Path::new("tools")
    });

    let script_path = tools_dir_path.join(format!("{}.tcl", tool_call.name));

    if !script_path.exists() {
        return Err(anyhow!("Tool not found: {}", tool_call.name));
    }

    // Parse arguments from JSON
    let args_json: serde_json::Value = serde_json::from_str(&tool_call.arguments)
        .map_err(|e| anyhow!("Failed to parse tool arguments: {}", e))?;

    // Convert JSON object to positional arguments based on tool info
    let positional_args = if let Some(obj) = args_json.as_object() {
        // Get tool info to determine parameter order
        let tool_info = super::tools::get_tool_info(
            &tool_call.name,
            tools_dir.map(|p| p.to_str()).unwrap_or(None)
        ).map_err(|e| anyhow!("Failed to get tool info: {}", e))?;

        let mut positional = Vec::new();
        for (param_name, param_info) in &tool_info.parameters {
            if let Some(value) = obj.get(param_name) {
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => serde_json::to_string(value)?,
                };
                positional.push(value_str);
            } else if param_info.required {
                return Err(anyhow!("Missing required parameter: {}", param_name));
            }
            // Optional parameter not provided - skip
        }
        positional
    } else {
        // Arguments is not an object, use as-is (array or single value)
        Vec::new()
    };

    // Build TCL command
    let mut cmd = format!("source {{{}}}\n", script_path.display());
    cmd.push_str("execute");
    for arg in &positional_args {
        cmd.push_str(&format!(" {}", quote_tcl_arg(arg)));
    }

    // Use rtcl to execute
    use rtcl_core::Interp;

    let mut interp = Interp::new();
    let result = interp.eval(&cmd)
        .map_err(|e| anyhow!("Tool execution failed: {}", e))?;

    Ok(result.as_str().to_string())
}

/// Quote a TCL argument for safe use in commands
fn quote_tcl_arg(s: &str) -> String {
    if s.is_empty() || !s.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | ';' | '"' | '\\' | '[' | ']' | '$' | '{' | '}')) {
        return s.to_string();
    }
    format!("{{{}}}", s.replace('}', "\\}"))
}

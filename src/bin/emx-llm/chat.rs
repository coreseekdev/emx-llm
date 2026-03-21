//! Chat command implementation

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};
use emx_llm::{create_client, create_client_for_model, load_with_default, load_tools_from_dir, validate_session_name, ProviderConfig, Session, Usage, ToolCall};
use futures::StreamExt;

/// Run the chat command
#[allow(clippy::too_many_arguments)]
pub async fn run(
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
    // Step 1: Validate session name is safe (before creating any files)
    validate_session_name(&session_name)?;

    // Step 2: Resolve and validate prompt (before creating any files)
    let prompt_text = resolve_prompt(prompt)?;
    if prompt_text.trim().is_empty() {
        return Err(anyhow!("prompt is empty; provide PROMPT or stdin content"));
    }

    // Step 3: Now that prompt is validated, create the session
    let (client, model_id) = resolve_client(model.as_deref(), api_base.as_deref())?;

    let mut session = Session::open(&session_name)?;
    let system_prompt = match system {
        Some(value) => Some(resolve_input_value(&value)?),
        None => None,
    };

    session.ensure_system_prompt(system_prompt.as_deref())?;

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
        let mut total_usage = Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
        let mut current_messages = messages;

        const MAX_TOOL_ROUNDS: usize = 10;
        for _round in 0..MAX_TOOL_ROUNDS {
            let mut response_stream = client.chat_stream(&current_messages, &model_id, tools_ref);
            let mut full_response = String::new();
            let mut round_usage: Option<Usage> = None;
            let mut round_tool_calls: Option<Vec<ToolCall>> = None;

            while let Some(event) = response_stream.next().await {
                match event {
                    Ok(event) => {
                        print!("{}", event.delta);
                        io::stdout().flush()?;
                        full_response.push_str(&event.delta);
                        if event.done {
                            round_usage = event.usage;
                            round_tool_calls = event.tool_calls;
                        }
                    }
                    Err(e) => {
                        eprintln!("Stream error: {}", e);
                        break;
                    }
                }
            }

            let usage = round_usage.unwrap_or(Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });
            total_usage.prompt_tokens += usage.prompt_tokens;
            total_usage.completion_tokens += usage.completion_tokens;
            total_usage.total_tokens += usage.total_tokens;

            if let Some(calls) = round_tool_calls {
                println!("\n[Tool Calls: {}]", calls.len());
                for (i, call) in calls.iter().enumerate() {
                    println!("  [{}] {}: {}", i + 1, call.name, call.arguments);
                }

                session.add_assistant_tool_calls(
                    calls.clone(),
                    &model_id,
                    &usage,
                    Some(started.elapsed().as_millis()),
                )?;

                for call in &calls {
                    let result = match execute_tool_call(call, tools_dir.as_ref()) {
                        Ok(r) => r,
                        Err(e) => {
                            // Return error message to LLM instead of crashing
                            format!("Error: {}", e)
                        }
                    };
                    if raw {
                        println!("\n[Tool Result: {}]\n{}", call.name, result);
                    } else {
                        println!("[Executed: {}]", call.name);
                    }
                    session.add_tool_result(call.id.clone(), result)?;
                }

                current_messages = session.messages().to_vec();
                continue; // Next round
            }

            // No tool calls — final text response
            if !full_response.is_empty() {
                session.add_assistant_response(
                    full_response,
                    &model_id,
                    &usage,
                    Some(started.elapsed().as_millis()),
                )?;
            }

            if token_stats {
                println!();
                println!("=== Token Stats ===");
                println!("Prompt tokens: {}", total_usage.prompt_tokens);
                println!("Completion tokens: {}", total_usage.completion_tokens);
                println!("Total tokens: {}", total_usage.total_tokens);
                println!("Duration (ms): {}", started.elapsed().as_millis());
            }
            break;
        }
    } else {
        // Non-streaming mode with tool call loop
        let started = Instant::now();
        let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
        let mut total_usage = Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
        let mut current_messages = messages;

        const MAX_TOOL_ROUNDS: usize = 10;
        for _round in 0..MAX_TOOL_ROUNDS {
            let (response, tool_calls, usage) = client.chat(&current_messages, &model_id, tools_ref).await?;
            total_usage.prompt_tokens += usage.prompt_tokens;
            total_usage.completion_tokens += usage.completion_tokens;
            total_usage.total_tokens += usage.total_tokens;

            if let Some(calls) = tool_calls {
                println!("[Tool Calls: {}]", calls.len());
                for (i, call) in calls.iter().enumerate() {
                    println!("  [{}] {}: {}", i + 1, call.name, call.arguments);
                }

                session.add_assistant_tool_calls(
                    calls.clone(),
                    &model_id,
                    &usage,
                    Some(started.elapsed().as_millis()),
                )?;

                for call in &calls {
                    let result = match execute_tool_call(call, tools_dir.as_ref()) {
                        Ok(r) => r,
                        Err(e) => {
                            // Return error message to LLM instead of crashing
                            format!("Error: {}", e)
                        }
                    };
                    if raw {
                        println!("\n[Tool Result: {}]\n{}", call.name, result);
                    } else {
                        println!("[Executed: {}]", call.name);
                    }
                    session.add_tool_result(call.id.clone(), result)?;
                }

                current_messages = session.messages().to_vec();
                continue; // Next round
            }

            // No tool calls — final text response
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
                println!("Prompt tokens: {}", total_usage.prompt_tokens);
                println!("Completion tokens: {}", total_usage.completion_tokens);
                println!("Total tokens: {}", total_usage.total_tokens);
                println!("Duration (ms): {}", started.elapsed().as_millis());
            }
            break;
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
    let args_json: serde_json::Value = serde_json::from_str(&tool_call.arguments)
        .map_err(|e| anyhow!("Failed to parse tool arguments: {}", e))?;

    let dir_str = tools_dir.and_then(|p| p.to_str());
    super::tools::call_tool_json(&tool_call.name, &args_json, dir_str)
}

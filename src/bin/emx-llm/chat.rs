//! Chat command implementation

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};
use emx_llm::{create_client, create_client_for_model, load_with_default, ProviderConfig, Session, Usage};
use futures::StreamExt;

/// Run the chat command
#[allow(clippy::too_many_arguments)]
pub fn run(
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
) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
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
        )
        .await
    })
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
                emx_llm::MessageRole::System => println!("  [System]: {}", msg.content),
                emx_llm::MessageRole::User => println!("  [User]: {}", msg.content),
                emx_llm::MessageRole::Assistant => println!("  [Assistant]: {}", msg.content),
            }
        }
        println!();
        println!("Total: {} messages", messages.len());
        return Ok(());
    }

    let messages = session
        .add_user_message(prompt_text, &attach)?
        .to_vec();

    let use_stream = stream || !no_stream;

    if use_stream {
        let started = Instant::now();
        let mut response_stream = client.chat_stream(&messages, &model_id);
        let mut full_response = String::new();
        let mut final_usage: Option<Usage> = None;

        while let Some(event) = response_stream.next().await {
            match event {
                Ok(event) => {
                    print!("{}", event.delta);
                    io::stdout().flush()?;

                    full_response.push_str(&event.delta);

                    if event.done {
                        println!();
                        final_usage = event.usage;
                    }
                }
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    break;
                }
            }
        }

        if !full_response.is_empty() {
            let usage = final_usage.unwrap_or(Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });
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
        let started = Instant::now();
        let (response, usage) = client.chat(&messages, &model_id).await?;
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

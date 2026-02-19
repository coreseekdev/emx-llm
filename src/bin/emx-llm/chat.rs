//! Chat command implementation

use anyhow::Result;
use emx_llm::{create_client, create_client_for_model, Message, MessageRole};
use futures::StreamExt;
use std::io::{self, Read, Write};

/// Default system prompt used when no explicit system prompt is provided
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful, harmless, and honest AI assistant.";

/// Simple txtar archive for parsing stdin
struct TxtarEntry {
    name: String,
    content: String,
}

struct TxtarArchive {
    comment: String,
    files: Vec<TxtarEntry>,
}

impl TxtarArchive {
    /// Parse txtar format from string
    /// Format: comment followed by files with -- filename -- headers
    fn parse(input: &str) -> Result<Self> {
        let lines: Vec<&str> = input.lines().collect();
        let mut comment = String::new();
        let mut files = Vec::new();
        let mut current_file: Option<&mut TxtarEntry> = None;

        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            if line.starts_with("-- ") && line.ends_with(" --") {
                // New file header
                let name = line[3..line.len() - 3].trim().to_string();
                files.push(TxtarEntry {
                    name,
                    content: String::new(),
                });
                // Safe: we just pushed to files, so last_mut() will return Some
                current_file = files.last_mut();
            } else if let Some(ref mut file) = current_file {
                file.content.push_str(line);
                file.content.push('\n');
            } else {
                // Part of comment
                if !comment.is_empty() {
                    comment.push('\n');
                }
                comment.push_str(line);
            }
            i += 1;
        }

        Ok(TxtarArchive { comment, files })
    }

    fn is_txtar(input: &str) -> bool {
        input.lines().any(|line| line.starts_with("-- ") && line.ends_with(" --"))
    }
}

/// Run the chat command
pub fn run(
    model: Option<String>,
    _api_base: Option<String>,
    stream: bool,
    prompts: Vec<String>,
    dry_run: bool,
    token_stats: bool,
    query: Vec<String>,
) -> Result<()> {
    // Use tokio runtime for async operations
    tokio::runtime::Runtime::new()?.block_on(async {
        run_async(model, stream, prompts, dry_run, token_stats, query).await
    })
}

async fn run_async(
    model: Option<String>,
    stream: bool,
    prompts: Vec<String>,
    dry_run: bool,
    token_stats: bool,
    query: Vec<String>,
) -> Result<()> {
    // Determine provider and model from hierarchical lookup
    let (client, model_id) = if let Some(model_ref) = &model {
        // Use hierarchical configuration lookup
        match create_client_for_model(model_ref) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Hierarchical model lookup failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // No model specified, use default configuration
        let config = emx_llm::load_with_default()?;
        let model_id = config.model.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No model configured. Set llm.provider.model"))?
            .clone();
        (create_client(config)?, model_id)
    };

    // Build messages from prompts and query
    let mut messages = Vec::new();

    // Collect explicit system prompts
    let mut explicit_system_prompts = Vec::new();

    for prompt_file in &prompts {
        if let Ok(content) = std::fs::read_to_string(prompt_file) {
            explicit_system_prompts.push(content);
        }
    }

    // Handle query: if no args provided, read from stdin
    let (query_text, attachments) = if query.is_empty() {
        // Read from stdin as txtar format (comment + files)
        let stdin = io::stdin();
        let mut buffer = String::new();
        stdin.lock().read_to_string(&mut buffer)?;
        let buffer = buffer.trim();

        // Try to parse as txtar
        if TxtarArchive::is_txtar(buffer) {
            let archive = TxtarArchive::parse(buffer)?;
            let files: Vec<String> = archive.files.iter()
                .map(|f| format!("-- {} --\n{}", f.name, f.content))
                .collect();
            (archive.comment.trim().to_string(), files)
        } else {
            // Not a valid txtar, treat as plain text
            (buffer.to_string(), Vec::new())
        }
    } else {
        (query.join(" "), Vec::new())
    };

    // Add attachments as context messages
    for attachment in &attachments {
        explicit_system_prompts.push(attachment.clone());
    }

    // Determine if we should use default system prompt
    let use_default_system = explicit_system_prompts.is_empty();

    // Add system messages
    if use_default_system {
        messages.push(Message {
            role: MessageRole::System,
            content: DEFAULT_SYSTEM_PROMPT.to_string(),
        });
    } else {
        for prompt in &explicit_system_prompts {
            messages.push(Message {
                role: MessageRole::System,
                content: prompt.clone(),
            });
        }
    }

    // Add the user query
    messages.push(Message {
        role: MessageRole::User,
        content: query_text.clone(),
    });

    if dry_run {
        // Output constructed messages without sending
        println!("=== Dry Run Mode ====");
        println!("API Base: {}", client.api_base());
        println!("Model: {}", model_id);
        println!("Max Tokens: {}", client.max_tokens());
        println!();

        // Separate system messages from conversation (Anthropic-style)
        let (system_msgs, conversation): (Vec<_>, Vec<_>) = messages
            .iter()
            .partition(|m| m.role == MessageRole::System);

        // Show system prompt(s) - no distinction between default and user-provided
        if system_msgs.len() == 1 {
            println!("System: {}", system_msgs[0].content);
        } else if !system_msgs.is_empty() {
            println!("System (combined):");
            for msg in &system_msgs {
                println!("---");
                println!("{}", msg.content);
            }
        }
        println!();

        // Show conversation messages
        println!("Messages:");
        for msg in &conversation {
            match msg.role {
                MessageRole::User => println!("  [User]: {}", msg.content),
                MessageRole::Assistant => println!("  [Assistant]: {}", msg.content),
                MessageRole::System => {} // Already shown above
            }
        }
        println!();
        println!("Total: {} system + {} conversation messages", system_msgs.len(), conversation.len());
        return Ok(());
    }

    // Send request
    if stream {
        let mut stream = client.chat_stream(&messages, &model_id);
        let mut full_response = String::new();
        let mut final_usage: Option<emx_llm::Usage> = None;

        while let Some(event) = stream.next().await {
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

        if token_stats {
            if let Some(usage) = final_usage {
                println!();
                println!("=== Token Stats ===");
                println!("Prompt tokens: {}", usage.prompt_tokens);
                println!("Completion tokens: {}", usage.completion_tokens);
                println!("Total tokens: {}", usage.total_tokens);
            }
        }
    } else {
        let (response, usage) = client.chat(&messages, &model_id).await?;
        println!("{}", response);

        if token_stats {
            println!();
            println!("=== Token Stats ===");
            println!("Prompt tokens: {}", usage.prompt_tokens);
            println!("Completion tokens: {}", usage.completion_tokens);
            println!("Total tokens: {}", usage.total_tokens);
        }
    }

    Ok(())
}

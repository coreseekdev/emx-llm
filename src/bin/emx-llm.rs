use anyhow::Result;
use clap::{Parser, Subcommand};
use emx_llm::{create_client, create_client_for_model, Message, ProviderType, MessageRole};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use tracing::info;

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

#[derive(Parser)]
#[command(name = "emx-llm")]
#[command(about = "LLM client for EMX with txtar support", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a chat completion request
    Chat {
        /// Model to use (can be qualified: e.g., "anthropic.glm.glm-5", "glm-5")
        #[arg(short, long)]
        model: Option<String>,

        /// API base URL (overrides default)
        #[arg(long)]
        api_base: Option<String>,

        /// Enable streaming output
        #[arg(short, long)]
        stream: bool,

        /// System prompt files (can be specified multiple times)
        #[arg(long = "prompt")]
        prompts: Vec<String>,

        /// Enable dry run mode (output prompt without sending to API)
        #[arg(long)]
        dry_run: bool,

        /// Show token usage statistics after response
        #[arg(long)]
        token_stats: bool,

        /// Query text
        query: Vec<String>,
    },

    /// Test configuration and API key
    Test {
        /// Provider type (openai or anthropic)
        #[arg(short, long, default_value = "openai")]
        provider: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            model,
            api_base: _,
            stream,
            prompts,
            dry_run,
            token_stats,
            query,
        } => {
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
        }
        Commands::Test { provider } => {
            let provider_type = match provider.to_lowercase().as_str() {
                "openai" => ProviderType::OpenAI,
                "anthropic" => ProviderType::Anthropic,
                _ => {
                    eprintln!("Unknown provider: {}", provider);
                    eprintln!("Supported providers: openai, anthropic");
                    std::process::exit(1);
                }
            };

            info!("Testing configuration for provider: {:?}", provider_type);

            // Build args to set provider type with fully nested structure
            let mut args = HashMap::new();
            let mut provider_table = toml::value::Table::new();
            provider_table.insert("type".to_string(), toml::Value::String(provider.to_lowercase()));
            let mut llm_table = toml::value::Table::new();
            llm_table.insert("provider".to_string(), toml::Value::Table(provider_table));
            args.insert("llm".to_string(), toml::Value::Table(llm_table));

            match emx_llm::ProviderConfig::load_with_args(Some(args)) {
                Ok(config) => {
                    println!("Configuration loaded successfully:");
                    println!("  Provider: {:?}", config.provider_type);
                    println!("  API Base: {}", config.api_base);
                    println!("  API Key: {}***", &config.api_key[..8.min(config.api_key.len())]);
                    if let Some(model) = &config.model() {
                        println!("  Default Model: {}", model);
                    }
                    println!();
                    println!("Configuration is valid!");
                }
                Err(e) => {
                    eprintln!("Configuration error: {}", e);
                    eprintln!();
                    eprintln!("Make sure to set up your config.toml or environment variables:");
                    eprintln!();
                    eprintln!("config.toml:");
                    eprintln!("  [llm.provider]");
                    eprintln!("  type = \"{}\"", provider);
                    eprintln!();
                    eprintln!("  [llm.provider.{}]", provider);
                    eprintln!("  api_base = \"...\"");
                    eprintln!("  api_key = \"...\"");
                    eprintln!("  model = \"...\"");
                    eprintln!();
                    eprintln!("Or set environment variables:");
                    match provider_type {
                        ProviderType::OpenAI => {
                            eprintln!("  export OPENAI_API_KEY=\"...\"");
                            eprintln!("  export OPENAI_API_BASE=\"...\"");
                        }
                        ProviderType::Anthropic => {
                            eprintln!("  export ANTHROPIC_AUTH_TOKEN=\"...\"");
                            eprintln!("  export ANTHROPIC_BASE_URL=\"...\"");
                        }
                    }
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

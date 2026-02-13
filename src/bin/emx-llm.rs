use anyhow::Result;
use clap::{Parser, Subcommand};
use emx_llm::{create_client, Message, ProviderType};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use tracing::info;

#[derive(Parser)]
#[command(name = "emx-llm")]
#[command(about = "LLM client for EMX with support for multiple providers", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a chat completion request
    Chat {
        /// Provider type (openai or anthropic)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,

        /// API base URL (overrides default)
        #[arg(long)]
        api_base: Option<String>,

        /// Enable streaming output
        #[arg(short, long)]
        stream: bool,

        /// System prompt file
        #[arg(long)]
        prompt: Option<String>,

        /// Query text (if omitted, enters interactive mode)
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
            provider,
            model,
            api_base,
            stream,
            prompt,
            query,
        } => {
            // Build CLI args override map with nested structure
            let mut args = HashMap::new();

            // Create fully nested structure: llm.provider.{type,model}
            let mut provider_table = toml::value::Table::new();
            if let Some(ref prov) = provider {
                provider_table.insert("type".to_string(), toml::Value::String(prov.clone()));
            }
            if let Some(ref modl) = model {
                provider_table.insert("model".to_string(), toml::Value::String(modl.clone()));
            }

            // Create llm table
            let mut llm_table = toml::value::Table::new();
            if !provider_table.is_empty() {
                llm_table.insert("provider".to_string(), toml::Value::Table(provider_table));
            }

            // Create provider-specific nested structure
            if let Some(ref prov) = provider {
                let provider_key = match prov.to_lowercase().as_str() {
                    "anthropic" => "anthropic",
                    _ => "openai",
                };
                let mut specific_table = toml::value::Table::new();
                if let Some(ref api_base) = api_base {
                    specific_table.insert("api_base".to_string(), toml::Value::String(api_base.clone()));
                }
                if !specific_table.is_empty() {
                    llm_table.insert(provider_key.to_string(), toml::Value::Table(specific_table));
                }
            }

            if !llm_table.is_empty() {
                args.insert("llm".to_string(), toml::Value::Table(llm_table));
            }

            // Load config with CLI args overrides
            let config = emx_llm::ProviderConfig::load_with_args(Some(args))?;

            let model = model.or_else(|| config.default_model.clone());

            let model = match model {
                Some(m) => m,
                None => {
                    eprintln!(
                        "Error: Model not specified. Use --model or configure in config.toml"
                    );
                    eprintln!("Example config.toml:");
                    eprintln!("  [llm.provider]");
                    eprintln!("  type = \"openai\"");
                    eprintln!("  model = \"gpt-4\"");
                    eprintln!();
                    eprintln!("  [llm.provider.openai]");
                    eprintln!("  api_key = \"sk-...\"");
                    eprintln!("  default_model = \"gpt-4\"");
                    std::process::exit(1);
                }
            };

            // Load system prompt if provided
            let system_prompt = if let Some(prompt_file) = prompt {
                Some(std::fs::read_to_string(prompt_file)?)
            } else {
                None
            };

            // Create client
            let client = create_client(config)?;

            if query.is_empty() {
                // Interactive mode
                run_interactive(client, &model, stream, system_prompt).await?;
            } else {
                // Single query mode
                let query_text = query.join(" ");
                let mut messages = Vec::new();

                if let Some(prompt) = system_prompt {
                    messages.push(Message::system(prompt));
                }

                messages.push(Message::user(query_text));

                if stream {
                    let mut stream = client.chat_stream(&messages, &model);
                    let mut full_response = String::new();

                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(event) => {
                                print!("{}", event.delta);
                                io::stdout().flush()?;
                                full_response.push_str(&event.delta);

                                if event.done {
                                    println!();
                                }
                            }
                            Err(e) => {
                                eprintln!("Stream error: {}", e);
                                break;
                            }
                        }
                    }
                } else {
                    let (response, _usage) = client.chat(&messages, &model).await?;
                    println!("{}", response);
                }
            }
        }
        Commands::Test { provider } => {
            let provider_type = match provider.to_lowercase().as_str() {
                "openai" | "openai" => ProviderType::OpenAI,
                "anthropic" | "Anthropic" => ProviderType::Anthropic,
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
                    if let Some(model) = &config.default_model {
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
                    eprintln!("  default_model = \"...\"");
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

async fn run_interactive(
    client: Box<dyn emx_llm::Client>,
    default_model: &str,
    stream: bool,
    system_prompt: Option<String>,
) -> Result<()> {
    println!("EMX LLM Interactive Mode");
    println!("Press Ctrl+D or Ctrl+C to exit");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut history: Vec<Message> = Vec::new();

    if let Some(ref prompt) = system_prompt {
        history.push(Message::system(prompt.clone()));
    }

    loop {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        let bytes_read = stdin.lock().read_line(&mut input)?;

        if bytes_read == 0 {
            // EOF (Ctrl+D)
            println!();
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            break;
        }

        if input == "clear" {
            history.clear();
            if let Some(prompt) = &system_prompt {
                history.push(Message::system(prompt.clone()));
            }
            println!("History cleared.");
            continue;
        }

        history.push(Message::user(input.to_string()));

        if stream {
            print!("\x1b[90m"); // Dim color for assistant response
            let mut stream = client.chat_stream(&history, default_model);
            let mut response = String::new();

            while let Some(event) = stream.next().await {
                match event {
                    Ok(event) => {
                        print!("{}", event.delta);
                        io::stdout().flush()?;
                        response.push_str(&event.delta);

                        if event.done {
                            print!("\x1b[0m"); // Reset color
                            println!();
                        }
                    }
                    Err(e) => {
                        print!("\x1b[0m");
                        eprintln!("\nStream error: {}", e);
                        break;
                    }
                }
            }

            history.push(Message::assistant(response));
        } else {
            let (response, _usage) = client.chat(&history, default_model).await?;
            println!("\x1b[90m{}\x1b[0m", response); // Dim color
            history.push(Message::assistant(response));
        }

        println!();
    }

    Ok(())
}

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

    /// Collect environment context for LLM inference
    Env {
        /// Output format: text, json, md (default: md)
        #[arg(long, default_value = "md")]
        format: String,

        /// Include directory listing
        #[arg(short, long)]
        files: bool,

        /// Include git status (if in a git repo)
        #[arg(short, long)]
        git: bool,

        /// Include environment variables (safe ones only)
        #[arg(short, long)]
        env_vars: bool,

        /// Include all information (shorthand for --files --git --env-vars)
        #[arg(short, long)]
        all: bool,

        /// Show file/directory size
        #[arg(long)]
        size: bool,

        /// Show file/directory modified time
        #[arg(long)]
        mtime: bool,

        /// Show file/directory created time
        #[arg(long)]
        ctime: bool,

        /// Show all file metadata (shorthand for --size --mtime --ctime)
        #[arg(long)]
        full: bool,

        /// Show ALL environment variables (includes sensitive ones, full PATH)
        #[arg(short, long)]
        verbose: bool,
    },
}

/// Metadata display options
struct MetadataOptions {
    show_size: bool,
    show_mtime: bool,
    show_ctime: bool,
}

/// Collect environment context for LLM inference
fn run_env_context(
    format: String,
    include_files: bool,
    include_git: bool,
    include_env: bool,
    meta_opts: MetadataOptions,
    verbose_env: bool,
) -> Result<()> {
    use std::env;

    // Collect basic system info
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let current_dir = env::current_dir()?;
    let current_dir_str = current_dir.display().to_string();
    let shell = env::var("SHELL")
        .or_else(|_| env::var("COMSPEC"))
        .or_else(|_| env::var("PSModulePath").map(|_| "powershell".to_string()))
        .unwrap_or_else(|_| "unknown".to_string());

    // Build context
    let mut sections: Vec<(&str, String)> = Vec::new();

    // Basic system info
    let mut system_info = String::new();
    system_info.push_str(&format!("os: {}\n", os));
    system_info.push_str(&format!("arch: {}\n", arch));
    system_info.push_str(&format!("shell: {}\n", shell));
    system_info.push_str(&format!("pwd: {}\n", current_dir_str));
    sections.push(("system", system_info));

    // Directory listing
    if include_files {
        let (dirs_section, files_section) = collect_file_listing(&current_dir, &meta_opts, &format)?;
        if !dirs_section.is_empty() {
            sections.push(("directories", dirs_section));
        }
        if !files_section.is_empty() {
            sections.push(("files", files_section));
        }
    }

    // Git status
    if include_git {
        let git_dir = current_dir.join(".git");
        if git_dir.exists() {
            let git_info = collect_git_info(&current_dir);
            sections.push(("git", git_info));
        }
    }

    // Environment variables
    if include_env || verbose_env {
        let env_info = collect_env_vars(verbose_env);
        sections.push(("env", env_info));
    }

    // Output based on format
    match format.as_str() {
        "json" => {
            let mut result = serde_json::Map::new();
            result.insert("os".to_string(), serde_json::json!(os));
            result.insert("arch".to_string(), serde_json::json!(arch));
            result.insert("shell".to_string(), serde_json::json!(shell));
            result.insert("pwd".to_string(), serde_json::json!(current_dir_str));

            for (name, content) in &sections {
                if *name != "system" {
                    result.insert(name.to_string(), serde_json::json!(content));
                }
            }

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "text" => {
            for (name, content) in &sections {
                println!("=== {} ===", name.to_uppercase());
                println!("{}", content);
            }
        }
        "md" | _ => {
            // Use markdown format
            println!("> **ENVIRONMENT CONTEXT REPORT**");
            println!("> For LLM inference context. Use `-v` for verbose output.");
            println!();

            for (name, content) in &sections {
                println!("## {}", name.to_uppercase());
                println!("{}", content);
            }
        }
    }

    Ok(())
}

/// Format file size in human-readable format
fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if size >= GB {
        format!("{:.1}GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1}MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1}KB", size as f64 / KB as f64)
    } else {
        format!("{}B", size)
    }
}

/// Format system time to readable string
fn format_system_time(time: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let datetime = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_else(|| chrono::Utc::now());
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Collect file and directory listing with metadata
fn collect_file_listing(
    dir: &std::path::Path,
    meta_opts: &MetadataOptions,
    format: &str,
) -> Result<(String, String)> {
    const MAX_ITEMS: usize = 50;

    let mut dirs: Vec<(String, u64, String, String)> = Vec::new(); // (name, size, modified, created)
    let mut files: Vec<(String, u64, String, String)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = metadata.len();
            let modified = metadata.modified()
                .map(format_system_time)
                .unwrap_or_else(|_| "unknown".to_string());
            let created = metadata.created()
                .map(format_system_time)
                .unwrap_or_else(|_| "unknown".to_string());

            if metadata.is_dir() {
                dirs.push((name, size, modified, created));
            } else {
                files.push((name, size, modified, created));
            }
        }
    }

    // Sort alphabetically (case-insensitive)
    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let total_dirs = dirs.len();
    let total_files = files.len();

    // Truncate if needed
    let dirs_truncated = dirs.len() > MAX_ITEMS;
    let files_truncated = files.len() > MAX_ITEMS;

    if dirs_truncated {
        dirs.truncate(MAX_ITEMS);
    }
    if files_truncated {
        files.truncate(MAX_ITEMS);
    }

    let use_markdown = format == "md";

    // Format directories
    let dirs_info = format_table(
        &dirs,
        total_dirs,
        dirs_truncated,
        MAX_ITEMS,
        meta_opts,
        use_markdown,
        true, // is_dir
    );

    // Format files
    let files_info = format_table(
        &files,
        total_files,
        files_truncated,
        MAX_ITEMS,
        meta_opts,
        use_markdown,
        false, // is_dir
    );

    Ok((dirs_info, files_info))
}

/// Format entries as a table (markdown or plain text)
fn format_table(
    entries: &[(String, u64, String, String)],
    total: usize,
    truncated: bool,
    max_items: usize,
    meta_opts: &MetadataOptions,
    use_markdown: bool,
    is_dir: bool,
) -> String {
    if entries.is_empty() {
        return if is_dir {
            "[No directories]\n".to_string()
        } else {
            "[No files]\n".to_string()
        };
    }

    // Check if any metadata columns are shown
    let has_metadata = (meta_opts.show_size && !is_dir) || meta_opts.show_mtime || meta_opts.show_ctime;

    let mut result = String::new();

    if has_metadata {
        // Use table format when metadata is shown
        let mut headers = vec!["Name"];
        if meta_opts.show_size && !is_dir {
            headers.push("Size");
        }
        if meta_opts.show_mtime {
            headers.push("Modified");
        }
        if meta_opts.show_ctime {
            headers.push("Created");
        }

        if use_markdown {
            // Markdown table header
            result.push_str(&format!("| {} |\n", headers.join(" | ")));
            result.push_str(&format!("| {} |\n", headers.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")));
        } else {
            // Plain text header
            result.push_str(&format!("# {}\n", headers.join(" | ")));
        }

        // Build rows
        for (name, size, modified, created) in entries {
            let mut cols = vec![name.clone()];
            if meta_opts.show_size && !is_dir {
                cols.push(format_size(*size));
            }
            if meta_opts.show_mtime {
                cols.push(modified.clone());
            }
            if meta_opts.show_ctime {
                cols.push(created.clone());
            }

            if use_markdown {
                result.push_str(&format!("| {} |\n", cols.join(" | ")));
            } else {
                result.push_str(&format!("{}\n", cols.join(" | ")));
            }
        }
    } else {
        // Simple list format when no metadata
        for (name, _, _, _) in entries {
            result.push_str(&format!("- {}\n", name));
        }
    }

    // Summary
    if truncated {
        result.push_str(&format!(
            "\n*[TRUNCATED: showing {} of {} {}]*\n",
            max_items,
            total,
            if is_dir { "directories" } else { "files" }
        ));
    } else {
        result.push_str(&format!(
            "\n*[Total: {} {}]*\n",
            total,
            if is_dir { "directories" } else { "files" }
        ));
    }

    result
}

/// Collect git information
fn collect_git_info(dir: &std::path::Path) -> String {
    let mut git_info = String::new();

    // Get remote URL
    if let Ok(output) = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
    {
        let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !remote.is_empty() {
            git_info.push_str(&format!("remote: {}\n", remote));
        }
    }

    // Get all local branches, mark current with *
    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--list"])
        .current_dir(dir)
        .output()
    {
        let branches = String::from_utf8_lossy(&output.stdout);
        if !branches.trim().is_empty() {
            git_info.push_str("branches:\n");
            for line in branches.lines() {
                let trimmed = line.trim();
                // git branch output: "* main" or "  feature"
                if trimmed.starts_with("* ") {
                    git_info.push_str(&format!("  * {} (current)\n", &trimmed[2..]));
                } else {
                    git_info.push_str(&format!("  - {}\n", trimmed));
                }
            }
        }
    }

    // Get all worktrees
    // Format: /path/to/worktree  COMMIT_HASH [BRANCH]
    // Get current worktree path first
    let current_wt_path = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    if let Ok(output) = std::process::Command::new("git")
        .args(["worktree", "list"])
        .current_dir(dir)
        .output()
    {
        let worktrees = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = worktrees.lines().collect();
        if !lines.is_empty() && !lines[0].is_empty() {
            git_info.push_str("worktrees:\n");

            for line in &lines {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    let wt_path = parts[0];
                    let branch_info = parts.iter()
                        .find(|p| p.starts_with('[') && p.ends_with(']'))
                        .map(|p| format!(" {}", p))
                        .unwrap_or_default();

                    // Check if this worktree is the current one
                    let is_current = current_wt_path.as_ref()
                        .map(|curr| {
                            // Normalize paths for comparison
                            let curr_normalized = curr.replace('\\', "/");
                            let wt_normalized = wt_path.replace('\\', "/");
                            curr_normalized == wt_normalized
                        })
                        .unwrap_or(false);

                    if is_current {
                        git_info.push_str(&format!("  * {}{} (current)\n", wt_path, branch_info));
                    } else {
                        git_info.push_str(&format!("  - {}{}\n", wt_path, branch_info));
                    }
                }
            }
        }
    }

    // Get status (short format)
    if let Ok(output) = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(dir)
        .output()
    {
        let status = String::from_utf8_lossy(&output.stdout);
        if !status.trim().is_empty() {
            git_info.push_str("status:\n");
            for line in status.lines() {
                git_info.push_str(&format!("  {}\n", line));
            }
        } else {
            git_info.push_str("status: clean\n");
        }
    }

    // Get recent commits
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(dir)
        .output()
    {
        let commits = String::from_utf8_lossy(&output.stdout);
        if !commits.trim().is_empty() {
            git_info.push_str("recent_commits:\n");
            for line in commits.lines() {
                git_info.push_str(&format!("  {}\n", line));
            }
        }
    }

    git_info
}

/// Collect environment variables
fn collect_env_vars(verbose: bool) -> String {
    use std::env;

    if verbose {
        // Show ALL environment variables
        let mut vars: Vec<(String, String)> = env::vars().collect();
        vars.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        let mut env_info = String::new();
        for (key, value) in vars {
            // Multi-line values: show first line + indication
            if value.contains('\n') {
                let first_line = value.lines().next().unwrap_or("");
                env_info.push_str(&format!("{}: {}...\n", key, first_line));
            } else {
                env_info.push_str(&format!("{}: {}\n", key, value));
            }
        }
        env_info
    } else {
        // Show only development-relevant variables (whitelist)
        let dev_vars = [
            // User/Shell
            "HOME", "USER", "USERNAME", "SHELL",
            "LANG", "TERM", "EDITOR", "VISUAL",
            "PWD", "OLDPWD",
            // Rust/Cargo
            "CARGO", "CARGO_HOME", "CARGO_PKG_NAME", "CARGO_PKG_VERSION",
            "RUSTUP_HOME", "RUSTUP_TOOLCHAIN",
            // Go
            "GOPATH", "GOROOT",
            // Node.js
            "NVM_HOME", "NVM_SYMLINK", "NODE_PATH",
            // Python
            "CONDA_PREFIX", "VIRTUAL_ENV", "PYTHONPATH",
            // Proxy (important for development)
            "http_proxy", "https_proxy", "all_proxy", "no_proxy",
            // MSYS2/MinGW (Windows development)
            "MSYSTEM", "MSYSTEM_PREFIX", "MINGW_PREFIX",
            // System info
            "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE",
        ];

        let mut env_info = String::new();
        for var in dev_vars {
            if let Ok(value) = env::var(var) {
                env_info.push_str(&format!("{}: {}\n", var, value));
            }
        }

        // Add PATH separately with truncation
        if let Ok(value) = env::var("PATH") {
            if value.len() > 200 {
                env_info.push_str(&format!("PATH: {}... [{} chars, use -v for full]\n", &value[..200], value.len()));
            } else {
                env_info.push_str(&format!("PATH: {}\n", value));
            }
        }

        env_info
    }
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
        Commands::Env {
            format,
            files,
            git,
            env_vars,
            all,
            size,
            mtime,
            ctime,
            full,
            verbose,
        } => {
            let include_files = files || all || verbose;
            let include_git = git || all || verbose;
            let include_env = env_vars || all || verbose;
            let meta_opts = MetadataOptions {
                show_size: size || full || verbose,
                show_mtime: mtime || full || verbose,
                show_ctime: ctime || full || verbose,
            };
            run_env_context(format, include_files, include_git, include_env, meta_opts, verbose)?;
        }
    }

    Ok(())
}

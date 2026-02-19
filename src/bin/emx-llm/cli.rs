//! CLI definitions for emx-llm

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "emx-llm")]
#[command(about = "LLM client for EMX with txtar support", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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

    /// Detect development environment (tools, versions, profiles)
    Dev {
        /// Show all profiles (not just detected ones)
        #[arg(short, long)]
        all: bool,

        /// Output format: text, json, md (default: md)
        #[arg(long, default_value = "md")]
        format: String,
    },
}

//! CLI definitions for emx-llm

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

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
        /// Session name (without .mbox suffix)
        session: String,

        /// Prompt text, or @file path
        prompt: Option<String>,

        /// Model to use (can be qualified: e.g., "anthropic.glm.glm-5", "glm-5")
        #[arg(short, long)]
        model: Option<String>,

        /// API base URL (overrides default)
        #[arg(long)]
        api_base: Option<String>,

        /// Enable streaming output
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_stream")]
        stream: bool,

        /// Disable streaming output
        #[arg(long = "no-stream", action = ArgAction::SetTrue, conflicts_with = "stream")]
        no_stream: bool,

        /// System prompt text, or @file path (only effective for new session)
        #[arg(short = 's', long)]
        system: Option<String>,

        /// Enable dry run mode (output prompt without sending to API)
        #[arg(long)]
        dry_run: bool,

        /// Show token usage statistics after response
        #[arg(long)]
        token_stats: bool,

        /// Attach files as context (repeatable)
        #[arg(long)]
        attach: Vec<PathBuf>,
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

    /// Manage and call TCL tools
    Tools {
        /// Show tool metadata (use with tool_name)
        #[arg(short, long)]
        info: bool,

        /// Show tool metadata as JSON
        #[arg(long)]
        json: bool,

        /// Tool name and parameters (e.g., glob --pattern "*.rs" --path src)
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Execute TCL scripts
    Exec {
        /// TCL script file to execute
        script: String,

        /// Script arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

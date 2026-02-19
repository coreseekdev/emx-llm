//! Dev command implementation - detect development environment profiles

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A development profile definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevProfile {
    /// Profile name
    pub name: String,
    /// Files to detect (any one triggers the profile)
    pub detect: Vec<String>,
    /// Tools to check versions
    pub tools: Vec<ToolDef>,
    /// Environment variables to show
    pub env_vars: Vec<String>,
}

/// Tool definition for version checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Tool name (for display)
    pub name: String,
    /// Command to run for version
    pub cmd: String,
}

/// Built-in profiles
fn get_builtin_profiles() -> Vec<DevProfile> {
    vec![
        // Rust
        DevProfile {
            name: "rust".to_string(),
            detect: vec!["Cargo.toml".to_string()],
            tools: vec![
                ToolDef { name: "rustc".to_string(), cmd: "rustc --version".to_string() },
                ToolDef { name: "cargo".to_string(), cmd: "cargo --version".to_string() },
                ToolDef { name: "rustup".to_string(), cmd: "rustup --version".to_string() },
            ],
            env_vars: vec![
                "RUSTUP_HOME".to_string(),
                "CARGO_HOME".to_string(),
                "RUSTUP_TOOLCHAIN".to_string(),
            ],
        },
        // Node.js
        DevProfile {
            name: "node".to_string(),
            detect: vec!["package.json".to_string()],
            tools: vec![
                ToolDef { name: "node".to_string(), cmd: "node --version".to_string() },
                ToolDef { name: "npm".to_string(), cmd: "npm --version".to_string() },
            ],
            env_vars: vec![
                "NODE_PATH".to_string(),
                "NVM_HOME".to_string(),
                "NVM_SYMLINK".to_string(),
            ],
        },
        // Python
        DevProfile {
            name: "python".to_string(),
            detect: vec![
                "pyproject.toml".to_string(),
                "setup.py".to_string(),
                "requirements.txt".to_string(),
            ],
            tools: vec![
                ToolDef { name: "python".to_string(), cmd: "python --version".to_string() },
                ToolDef { name: "python3".to_string(), cmd: "python3 --version".to_string() },
                ToolDef { name: "pip".to_string(), cmd: "pip --version".to_string() },
            ],
            env_vars: vec![
                "PYTHONPATH".to_string(),
                "VIRTUAL_ENV".to_string(),
                "CONDA_PREFIX".to_string(),
            ],
        },
        // Go
        DevProfile {
            name: "go".to_string(),
            detect: vec!["go.mod".to_string()],
            tools: vec![
                ToolDef { name: "go".to_string(), cmd: "go version".to_string() },
            ],
            env_vars: vec![
                "GOPATH".to_string(),
                "GOROOT".to_string(),
                "GOCACHE".to_string(),
            ],
        },
    ]
}

/// Detect which profiles are active in the given directory
fn detect_profiles(dir: &Path, show_all: bool) -> Vec<DevProfile> {
    let profiles = get_builtin_profiles();

    if show_all {
        return profiles;
    }

    profiles.into_iter().filter(|profile| {
        // Check if any detect file exists
        profile.detect.iter().any(|file| dir.join(file).exists())
    }).collect()
}

/// Get tool version by running command
fn get_tool_version(tool: &ToolDef) -> Option<String> {
    let parts: Vec<&str> = tool.cmd.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let output = std::process::Command::new(parts[0])
        .args(&parts[1..])
        .output()
        .ok()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }

    // Try stderr for some tools that output there
    let version = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !version.is_empty() {
        return Some(version);
    }

    None
}

/// Get environment variable value
fn get_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Run the dev command
pub fn run(show_all: bool, format: String) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let profiles = detect_profiles(&current_dir, show_all);

    if profiles.is_empty() {
        println!("No development profiles detected in current directory.");
        println!("Use --all to show all available profiles.");
        return Ok(());
    }

    let mut results: Vec<(&str, String)> = Vec::new();

    for profile in &profiles {
        let mut section = String::new();

        // Collect tool versions
        let mut tools_found = Vec::new();
        for tool in &profile.tools {
            if let Some(version) = get_tool_version(tool) {
                tools_found.push(format!("{}: {}", tool.name, version));
            }
        }

        if !tools_found.is_empty() {
            section.push_str("tools:\n");
            for tool_info in &tools_found {
                section.push_str(&format!("  - {}\n", tool_info));
            }
        }

        // Collect environment variables
        let mut env_found = Vec::new();
        for var in &profile.env_vars {
            if let Some(value) = get_env_var(var) {
                env_found.push(format!("{}: {}", var, value));
            }
        }

        if !env_found.is_empty() {
            section.push_str("env:\n");
            for env_info in &env_found {
                section.push_str(&format!("  - {}\n", env_info));
            }
        }

        if !section.is_empty() {
            results.push((&profile.name, section));
        }
    }

    // Output
    match format.as_str() {
        "json" => {
            let mut json_result = serde_json::Map::new();
            for (name, content) in &results {
                json_result.insert(name.to_string(), serde_json::json!(content));
            }
            println!("{}", serde_json::to_string_pretty(&json_result)?);
        }
        "text" => {
            for (name, content) in &results {
                println!("=== DEV: {} ===", name.to_uppercase());
                println!("{}", content);
            }
        }
        _ => {
            // Default: markdown format
            for (name, content) in &results {
                println!("## DEV: {}", name.to_uppercase());
                println!("{}", content);
            }
        }
    }

    Ok(())
}

//! Tools subcommand - manage and call TCL tools

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use rtcl_core::{Interp, Value};
use serde::Serialize;

/// Default tools directory
const DEFAULT_TOOLS_DIR: &str = "tools";

/// Tool metadata extracted from TCL script
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, ParameterInfo>,
    pub returns: Option<String>,
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterInfo {
    pub param_type: String,
    pub required: bool,
    pub description: String,
}

/// List all available tools in the tools directory
pub fn list_tools(tools_dir: Option<&str>) -> Result<Vec<String>> {
    let tools_dir = get_tools_dir(tools_dir)?;
    if !tools_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tools = Vec::new();
    for entry in fs::read_dir(&tools_dir)
        .with_context(|| format!("Failed to read tools directory: {}", tools_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("tcl") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                tools.push(name.to_string());
            }
        }
    }

    tools.sort();
    Ok(tools)
}

/// Get tool metadata by calling its info command
pub fn get_tool_info(tool_name: &str, tools_dir: Option<&str>) -> Result<ToolInfo> {
    let tools_dir = get_tools_dir(tools_dir)?;
    let script_path = tools_dir.join(format!("{}.tcl", tool_name));

    if !script_path.exists() {
        anyhow::bail!("Tool not found: {}", tool_name);
    }

    let mut interp = Interp::new();
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    let info_result = interp.eval("info")
        .context("Tool script must define 'info' command")?;

    parse_tool_info(&info_result, tool_name)
}

/// Parse tool info from TCL dict value
fn parse_tool_info(value: &Value, tool_name: &str) -> Result<ToolInfo> {
    let dict = value.as_dict()
        .context("info command must return a dict")?;

    let name = dict.get("name")
        .map(|v| v.as_str().to_string())
        .unwrap_or_else(|| tool_name.to_string());

    let description = dict.get("description")
        .map(|v| v.as_str().to_string())
        .context("Tool must have a description")?;

    let mut parameters = HashMap::new();
    if let Some(params_value) = dict.get("parameters") {
        if let Some(params_dict) = params_value.as_dict() {
            for (param_name, param_info) in params_dict {
                if let Some(info_dict) = param_info.as_dict() {
                    let param_type = info_dict.get("type")
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_else(|| "string".to_string());
                    let required = info_dict.get("required")
                        .map(|v| parse_tcl_bool(v.as_str()).unwrap_or(false))
                        .unwrap_or(false);
                    let description = info_dict.get("description")
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_else(|| "".to_string());

                    parameters.insert(param_name.clone(), ParameterInfo {
                        param_type,
                        required,
                        description,
                    });
                }
            }
        }
    }

    let returns = dict.get("returns")
        .map(|v| v.as_str().to_string());

    let example = dict.get("example")
        .map(|v| v.as_str().to_string());

    Ok(ToolInfo {
        name,
        description,
        parameters,
        returns,
        example,
    })
}

/// Parse a TCL boolean string
fn parse_tcl_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Call a tool with parameters
pub fn call_tool(tool_name: &str, params: &[String], tools_dir: Option<&str>) -> Result<String> {
    let tools_dir = get_tools_dir(tools_dir)?;
    let script_path = tools_dir.join(format!("{}.tcl", tool_name));

    if !script_path.exists() {
        anyhow::bail!("Tool not found: {}", tool_name);
    }

    let mut interp = Interp::new();
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    let mut cmd = format!("execute");
    for param in params {
        cmd.push_str(&format!(" {}", quote_tcl_arg(param)));
    }

    let result = interp.eval(&cmd)
        .context("Tool script must define 'execute' command")?;

    // Use rtcl's built-in json::encode to convert to JSON
    // Use 'list' schema to ensure proper array encoding
    interp.set_var("_tool_result", result)?;
    let json_result = interp.eval("json::encode $_tool_result list")?;
    Ok(json_result.as_str().to_string())
}

/// Get tools directory path
fn get_tools_dir(tools_dir: Option<&str>) -> Result<PathBuf> {
    if let Some(dir) = tools_dir {
        Ok(PathBuf::from(dir))
    } else {
        if let Ok(dir) = std::env::var("EMX_TOOLS_DIR") {
            return Ok(PathBuf::from(dir));
        }
        Ok(PathBuf::from(DEFAULT_TOOLS_DIR))
    }
}

/// Quote a TCL argument for safe use in commands
fn quote_tcl_arg(s: &str) -> String {
    if s.is_empty() || !s.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | ';' | '"' | '\\' | '[' | ']' | '$' | '{' | '}')) {
        return s.to_string();
    }
    format!("{{{}}}", s.replace('}', "\\}"))
}

/// Run the tools subcommand
pub fn run(
    tool_name: Option<String>,
    info: bool,
    json: bool,
    params: Vec<String>,
) -> Result<()> {
    match (tool_name, info, params.as_slice()) {
        // List all tools
        (None, _, []) => {
            let tools = list_tools(None)?;
            if json {
                println!("{}", serde_json::to_string(&tools)?);
            } else if tools.is_empty() {
                println!("No tools found in {}/ directory", DEFAULT_TOOLS_DIR);
            } else {
                println!("Available tools:");
                for tool in &tools {
                    println!("  - {}", tool);
                }
            }
        }
        // Show tool info
        (Some(name), true, []) | (Some(name), false, []) => {
            let tool_info = get_tool_info(&name, None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tool_info)?);
            } else {
                print_tool_info(&tool_info);
            }
        }
        // Call tool
        (Some(name), _, [params @ ..]) => {
            let result = call_tool(&name, params, None)?;
            println!("{}", result);
        }
        // Invalid: no tool name but params provided (shouldn't happen due to clap)
        (None, _, [_]) | (None, _, [_, _, ..]) => {
            anyhow::bail!("Tool name is required when providing parameters");
        }
        // Invalid: info with params (handled by clap, but rust needs exhaustive match)
        (Some(_), true, [_]) | (Some(_), true, [_, _, ..]) => {
            anyhow::bail!("Cannot use --info with --params");
        }
    }

    Ok(())
}

/// Print tool info in human-readable format
fn print_tool_info(info: &ToolInfo) {
    println!("Tool: {}", info.name);
    println!("Description: {}", info.description);

    if !info.parameters.is_empty() {
        println!("\nParameters:");
        for (name, param) in &info.parameters {
            let req_marker = if param.required { " (required)" } else { " (optional)" };
            println!("  --{} {}{}: {}", name, param.param_type, req_marker, param.description);
        }
    }

    if let Some(returns) = &info.returns {
        println!("\nReturns: {}", returns);
    }

    if let Some(example) = &info.example {
        println!("\nExample: {}", example);
    }
}

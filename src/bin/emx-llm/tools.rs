//! Tools subcommand - manage and call TCL tools

use anyhow::{Context, Result};

/// Convert an rtcl error to anyhow by stringifying it.
///
/// rtcl::Error contains `Value` (which uses `Rc`, not `Send+Sync`),
/// so it cannot be stored inside `anyhow::Error` directly.  Stringifying
/// at the boundary preserves the error message while satisfying anyhow's
/// `Send + Sync + 'static` requirement.
fn tcl_err(e: rtcl_core::Error) -> anyhow::Error {
    anyhow::anyhow!("{}", e)
}
use indexmap::IndexMap;
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
    pub parameters: IndexMap<String, ParameterInfo>,
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
        .map_err(tcl_err)
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    let info_result = interp.eval("info")
        .map_err(tcl_err)
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

    let mut parameters = IndexMap::new();
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

/// Parse named arguments (e.g., --pattern "*.rs" --path "src/")
/// and convert them to positional arguments based on tool parameter definitions.
fn parse_named_args(tool_name: &str, raw_args: &[String], tools_dir: Option<&str>) -> Result<Vec<String>> {
    let tool_info = get_tool_info(tool_name, tools_dir)?;

    // Build parameter order from tool info (preserves insertion order with IndexMap)
    let param_order: Vec<String> = tool_info.parameters.keys().cloned().collect();

    // Parse raw args into key-value pairs
    let mut parsed_args: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut i = 0;
    while i < raw_args.len() {
        let arg = &raw_args[i];
        if arg.starts_with("--") {
            let key = arg[2..].to_string();
            if i + 1 < raw_args.len() && !raw_args[i + 1].starts_with("--") {
                parsed_args.insert(key, raw_args[i + 1].clone());
                i += 2;
            } else {
                parsed_args.insert(key, String::new());
                i += 1;
            }
        } else {
            // Positional argument - use in order
            i += 1;
        }
    }

    // Convert to positional args in the order defined by tool info
    let mut positional_args: Vec<String> = Vec::new();
    for param_name in &param_order {
        if let Some(value) = parsed_args.get(param_name) {
            positional_args.push(value.clone());
        } else if let Some(param_info) = tool_info.parameters.get(param_name) {
            if param_info.required {
                anyhow::bail!("Missing required parameter: --{}", param_name);
            }
            // Optional parameter not provided - skip
        }
    }

    Ok(positional_args)
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
        .map_err(tcl_err)
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    let mut cmd = format!("execute");
    for param in params {
        cmd.push_str(&format!(" {}", quote_tcl_arg(param)));
    }

    let result = interp.eval(&cmd)
        .map_err(tcl_err)
        .context("Tool script must define 'execute' command")?;

    // Determine output format based on result type
    // For strings: return raw content (not JSON-encoded)
    // For dicts/lists: use JSON encoding
    match result.type_name() {
        "dict" | "list" => {
            // Use JSON encoding for complex types
            interp.set_var("_tool_result", result.clone()).map_err(tcl_err)?;
            let schema = if result.type_name() == "dict" { "obj" } else { "list" };
            let json_result = interp.eval(&format!("json::encode $_tool_result {}", schema)).map_err(tcl_err)?;
            Ok(json_result.as_str().to_string())
        }
        _ => {
            // For strings and primitives, return raw content
            Ok(result.as_str().to_string())
        }
    }
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
    info: bool,
    json: bool,
    args: Vec<String>,
) -> Result<()> {
    // Parse tool name from args
    let tool_name = args.first().map(|s| s.as_str());

    // Extract tool parameters (everything after the tool name)
    let tool_params: &[String] = if args.len() > 1 { &args[1..] } else { &[] };

    match (tool_name, info, tool_params.is_empty()) {
        // List all tools
        (None, _, true) => {
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
        (Some(name), _, true) => {
            let tool_info = get_tool_info(name, None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tool_info)?);
            } else {
                print_tool_info(&tool_info);
            }
        }
        // Call tool
        (Some(name), _, false) => {
            let positional_args = parse_named_args(name, tool_params, None)?;
            let result = call_tool(name, &positional_args, None)?;
            println!("{}", result);
        }
        // Invalid: no tool name but params provided
        (None, _, false) => {
            anyhow::bail!("Tool name is required when providing parameters");
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

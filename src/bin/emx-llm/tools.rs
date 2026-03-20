//! Tools subcommand - manage and call TCL tools

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use rtcl_core::{Interp, Value};

/// Default tools directory
const DEFAULT_TOOLS_DIR: &str = "tools";

/// Tool metadata extracted from TCL script
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, ParameterInfo>,
    pub returns: Option<String>,
    pub example: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub param_type: String,
    pub required: bool,
    pub description: String,
}

/// List all available tools in the tools directory
pub fn list_tools(tools_dir: Option<&str>) -> Result<Vec<String>> {
    let tools_dir = get_tools_dir(tools_dir)?;

    if !tools_dir.exists() {
        // Return empty list if tools directory doesn't exist
        return Ok(Vec::new());
    }

    let mut tools = Vec::new();

    for entry in fs::read_dir(&tools_dir)
        .with_context(|| format!("Failed to read tools directory: {}", tools_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Only process .tcl files
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

    // Create TCL interpreter and source the script
    let mut interp = Interp::new();
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    // Call the info command
    let info_result = interp.eval("info")
        .context("Tool script must define 'info' command")?;

    // Parse the info dict
    parse_tool_info(&info_result, tool_name)
}

/// Parse tool info from TCL dict value
fn parse_tool_info(value: &Value, tool_name: &str) -> Result<ToolInfo> {
    let info_str = value.to_str();
    let dict = parse_tcl_dict(&info_str)?;

    let name = dict.get("name")
        .map(|v| v.as_str())
        .unwrap_or(tool_name)
        .to_string();

    let description = dict.get("description")
        .map(|v| v.as_str())
        .context("Tool must have a description")?
        .to_string();

    let mut parameters = HashMap::new();
    if let Some(params_value) = dict.get("parameters") {
        let params_str = params_value.as_str();
        if let Ok(params_dict) = parse_tcl_dict(params_str) {
            for (param_name, param_info) in params_dict {
                let info_str = param_info.as_str();
                if let Ok(param_dict) = parse_tcl_dict(info_str) {
                    let param_type = param_dict.get("type")
                        .map(|v| v.as_str())
                        .unwrap_or("string")
                        .to_string();
                    let required = param_dict.get("required")
                        .and_then(parse_tcl_bool)
                        .unwrap_or(false);
                    let description = param_dict.get("description")
                        .map(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

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
        .map(|v| v.as_str())
        .map(|s| s.to_string());

    let example = dict.get("example")
        .map(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ToolInfo {
        name,
        description,
        parameters,
        returns,
        example,
    })
}

/// Parse a TCL dict string into a HashMap
fn parse_tcl_dict(s: &str) -> Result<HashMap<String, Value>> {
    let mut result = HashMap::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // Skip whitespace
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }

    if i < chars.len() && chars[i] == '{' {
        i += 1; // Skip opening brace
    }

    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        if i >= chars.len() {
            break;
        }

        if chars[i] == '}' {
            break;
        }

        // Parse key
        let (key, new_i) = parse_tcl_word(&chars, i)?;
        i = new_i;

        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        // Parse value
        let (value_str, new_i) = parse_tcl_word(&chars, i)?;
        i = new_i;

        result.insert(key, Value::from_str(&value_str));
    }

    Ok(result)
}

/// Parse a single TCL word (key or value in dict)
fn parse_tcl_word(chars: &[char], start: usize) -> Result<(String, usize)> {
    let mut i = start;
    let mut result = String::new();

    // Skip whitespace
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }

    if i >= chars.len() {
        return Ok((result, i));
    }

    match chars[i] {
        '{' => {
            // Braced string
            i += 1;
            let mut brace_depth = 1;
            while i < chars.len() && brace_depth > 0 {
                match chars[i] {
                    '{' => {
                        brace_depth += 1;
                        result.push('{');
                    }
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth > 0 {
                            result.push('}');
                        }
                    }
                    '\\' => {
                        result.push('\\');
                        if i + 1 < chars.len() {
                            i += 1;
                            result.push(chars[i]);
                        }
                    }
                    c => result.push(c),
                }
                i += 1;
            }
        }
        '"' => {
            // Quoted string
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // Skip closing quote
            }
        }
        _ => {
            // Unquoted word
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '}' {
                result.push(chars[i]);
                i += 1;
            }
        }
    }

    Ok((result, i))
}

/// Parse a TCL boolean value
fn parse_tcl_bool(value: &Value) -> Option<bool> {
    let s = value.to_str().to_lowercase();
    match s.as_str() {
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

    // Create TCL interpreter and source the script
    let mut interp = Interp::new();
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .with_context(|| format!("Failed to load tool script: {}", script_path.display()))?;

    // Build command with quoted arguments
    let mut cmd = format!("execute");
    for param in params {
        cmd.push_str(&format!(" {}", quote_tcl_arg(param)));
    }

    // Call the execute command
    let result = interp.eval(&cmd)
        .context("Tool script must define 'execute' command")?;

    // Convert result to JSON string
    // Tools return a dict, so parse it and use field hints
    let result_str = result.to_str();
    if let Ok(dict) = parse_tcl_dict(&result_str) {
        // It's a dict result from the tool
        dict_to_json_with_hints(&dict)
    } else {
        // Fallback to regular conversion
        value_to_json(&result)
    }
}

/// Get tools directory path
fn get_tools_dir(tools_dir: Option<&str>) -> Result<PathBuf> {
    if let Some(dir) = tools_dir {
        Ok(PathBuf::from(dir))
    } else {
        // Check EMX_TOOLS_DIR environment variable first
        if let Ok(dir) = std::env::var("EMX_TOOLS_DIR") {
            return Ok(PathBuf::from(dir));
        }

        // Use default tools directory
        Ok(PathBuf::from(DEFAULT_TOOLS_DIR))
    }
}

/// Quote a TCL argument for safe use in commands
fn quote_tcl_arg(s: &str) -> String {
    if s.is_empty() {
        return "{}".to_string();
    }

    // Check if we need braces
    let needs_bracing = s.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | ';' | '"' | '\\' | '[' | ']' | '$' | '{' | '}'));

    if needs_bracing {
        format!("{{{}}}", s.replace("}", "\\}"))
    } else {
        s.to_string()
    }
}

/// Convert TCL value to JSON string
/// field_hint provides context: "matches" -> list, "content" -> string, etc.
fn value_to_json_with_hint(value: &Value, field_hint: Option<&str>) -> Result<String> {
    let value_str = value.to_str();

    // Try to parse as list first (TCL lists are more common)
    if let Some(list) = value.as_list() {
        // Check if we should force list interpretation based on field name
        let force_list = match field_hint {
            Some("matches" | "files" | "items" | "results") => true,
            _ => false,
        };

        // Check if it's actually a dict (even number of elements, looks like key-value pairs)
        if !force_list && list.len() % 2 == 0 && list.len() >= 2 {
            // Try to interpret as dict
            let mut is_dict = true;
            for (i, item) in list.iter().enumerate() {
                // Dict keys should be simple strings
                let item_str = item.as_str();
                if i % 2 == 0 && (item_str.contains(' ') || item_str.contains('\t')) {
                    is_dict = false;
                    break;
                }
            }

            if is_dict {
                // Parse as dict
                let mut result = String::from("{");
                let mut first = true;
                for i in (0..list.len()).step_by(2) {
                    if !first {
                        result.push_str(",");
                    }
                    let key = list[i].as_str();
                    let val = &list[i + 1];
                    result.push_str(&format!("\"{}\":", escape_json(key)));
                    result.push_str(&value_to_json_with_hint(val, None)?);
                    first = false;
                }
                result.push_str("}");
                return Ok(result);
            }
        }

        // It's a list
        let mut result = String::from("[");
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                result.push_str(",");
            }
            result.push_str(&value_to_json_with_hint(item, None)?);
        }
        result.push_str("]");
        return Ok(result);
    }

    // Check if it's a number
    if let Some(n) = value.as_int() {
        return Ok(n.to_string());
    }

    if let Some(n) = value.as_float() {
        return Ok(n.to_string());
    }

    // Check if it's a boolean
    if let Some(b) = value.as_bool() {
        return Ok(if b { "true" } else { "false" }.to_string());
    }

    // Default: treat as string
    Ok(format!("\"{}\"", escape_json(&value_str)))
}

/// Convert TCL value to JSON string (without context hint)
fn value_to_json(value: &Value) -> Result<String> {
    value_to_json_with_hint(value, None)
}

/// Convert a TCL dict to JSON with field hints for proper type handling
fn dict_to_json_with_hints(dict: &HashMap<String, Value>) -> Result<String> {
    let mut result = String::from("{");
    let mut first = true;

    // Process in a predictable order
    let mut keys: Vec<_> = dict.keys().collect();
    keys.sort();

    for key in keys {
        if !first {
            result.push_str(",");
        }

        let val = &dict[key];
        result.push_str(&format!("\"{}\":", escape_json(key)));

        // Handle based on field name
        match key.as_str() {
            "matches" | "files" | "items" | "results" => {
                // Force list interpretation
                if let Some(list) = val.as_list() {
                    result.push_str("[");
                    for (i, item) in list.iter().enumerate() {
                        if i > 0 {
                            result.push_str(",");
                        }
                        let item_str = item.as_str();
                        result.push_str(&format!("\"{}\"", escape_json(item_str)));
                    }
                    result.push_str("]");
                } else {
                    // Fallback to string
                    result.push_str(&format!("\"{}\"", escape_json(val.as_str())));
                }
            }
            "content" | "data" | "text" => {
                // Force string interpretation
                result.push_str(&format!("\"{}\"", escape_json(val.as_str())));
            }
            _ => {
                // Try to infer type
                let val_str = val.as_str();
                if val.as_list().is_some() && val_str.contains(' ') {
                    // Likely a string with spaces
                    result.push_str(&format!("\"{}\"", escape_json(val_str)));
                } else {
                    // Use basic value conversion
                    if let Some(n) = val.as_int() {
                        result.push_str(&n.to_string());
                    } else if let Some(b) = val.as_bool() {
                        result.push_str(if b { "true" } else { "false" });
                    } else {
                        result.push_str(&format!("\"{}\"", escape_json(val_str)));
                    }
                }
            }
        }
        first = false;
    }

    result.push_str("}");
    Ok(result)
}

/// Escape string for JSON
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Run the tools subcommand
pub fn run(
    tool_name: Option<String>,
    info: bool,
    json: bool,
    params: Vec<String>,
) -> Result<()> {
    if let Some(name) = tool_name {
        if info {
            // Show tool info
            let tool_info = get_tool_info(&name, None)?;

            if json {
                // Output as JSON
                let json_output = tool_info_to_json(&tool_info);
                println!("{}", json_output);
            } else {
                // Output as human-readable text
                print_tool_info(&tool_info);
            }
        } else if !params.is_empty() {
            // Call the tool with parameters
            let result = call_tool(&name, &params, None)?;
            println!("{}", result);
        } else {
            // Show tool info by default
            let tool_info = get_tool_info(&name, None)?;
            if json {
                println!("{}", tool_info_to_json(&tool_info));
            } else {
                print_tool_info(&tool_info);
            }
        }
    } else {
        // List all tools
        let tools = list_tools(None)?;

        if json {
            // Output as JSON array
            let json_array = format!("[{}]",
                tools.iter()
                    .map(|t| format!("\"{}\"", escape_json(t)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("{}", json_array);
        } else {
            // Output as plain list
            if tools.is_empty() {
                println!("No tools found in {}/ directory", DEFAULT_TOOLS_DIR);
            } else {
                println!("Available tools:");
                for tool in &tools {
                    println!("  - {}", tool);
                }
            }
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

/// Convert tool info to JSON (for OpenAI/Anthropic function calling)
fn tool_info_to_json(info: &ToolInfo) -> String {
    let mut json = String::from("{");
    json.push_str(&format!("\"name\":\"{}\",", escape_json(&info.name)));
    json.push_str(&format!("\"description\":\"{}\",", escape_json(&info.description)));

    // Build parameters schema
    json.push_str("\"parameters\":{");
    json.push_str("\"type\":\"object\",");
    json.push_str("\"properties\":{");

    let mut param_entries = Vec::new();
    for (name, param) in &info.parameters {
        let entry = format!(
            "\"{}\":{{\"type\":\"{}\",\"description\":\"{}\"}}",
            escape_json(name),
            escape_json(&param.param_type),
            escape_json(&param.description)
        );
        param_entries.push(entry);
    }

    json.push_str(&param_entries.join(","));
    json.push_str("},");

    // Required parameters
    let required: Vec<&String> = info.parameters.iter()
        .filter(|(_, p)| p.required)
        .map(|(n, _)| n)
        .collect();

    if !required.is_empty() {
        json.push_str("\"required\":[");
        json.push_str(&required.iter()
            .map(|r| format!("\"{}\"", escape_json(r)))
            .collect::<Vec<_>>()
            .join(","));
        json.push_str("]");
    } else {
        json.push_str("\"required\":[]");
    }

    json.push_str("}"); // end parameters
    json.push_str("}"); // end object

    json
}

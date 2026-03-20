# Tools System

emx-llm includes a TCL-based tools system that allows you to create and call custom tools with named parameters.

## Overview

The tools system provides:
- **Tool discovery**: List available tools
- **Tool metadata**: Get parameter definitions and usage information
- **Named parameters**: Use `--key value` syntax for tool parameters
- **Type-aware output**: Automatic JSON encoding for complex types, raw content for strings
- **Recursive glob**: Support for `**/*.ext` patterns to search subdirectories

## Command Usage

```bash
# List all available tools
emx-llm tools

# Show tool information
emx-llm tools --info <tool_name>

# Show tool information as JSON
emx-llm tools --info <tool_name> --json

# Call a tool with parameters
emx-llm tools <tool_name> --<param1> <value1> --<param2> <value2>
```

## Available Tools

### glob

Find files matching a pattern.

```bash
# Recursively find all .rs files in current directory
emx-llm tools glob --pattern "**/*.rs"

# Find files in specific directory
emx-llm tools glob --pattern "*.rs" --path "src"

# Find all .tcl files
emx-llm tools glob --pattern "**/*.tcl"
```

**Parameters:**
- `--pattern` (required): Glob pattern (e.g., `*.rs`, `**/*.txt`)
- `--path` (optional): Base directory (default: current directory)

**Patterns:**
- `*.ext` - Non-recursive, matches files in specified directory only
- `**/*.ext` - Recursive, matches files in all subdirectories

**Returns:** JSON array of matching file paths

### read

Read the contents of a file.

```bash
# Read a file
emx-llm tools read --path Cargo.toml
```

**Parameters:**
- `--path` (required): File path to read

**Returns:** Raw file content (not JSON-encoded)

## Creating New Tools

Tools are TCL scripts stored in the `tools/` directory. Each tool must define two procedures:

### Tool Structure

```tcl
# tools/mytool.tcl

# Tool metadata
proc info {} {
    return [dict create \
        name mytool \
        description {Brief description of what the tool does} \
        parameters [dict create \
            input [dict create \
                type string \
                required true \
                description {Description of the input parameter} \
            ] \
            option [dict create \
                type string \
                required false \
                description {Description of optional parameter} \
            ] \
        ] \
        returns {Description of return value} \
        example {mytool --input "value" --option "value"} \
    ]
}

# Tool implementation
proc execute {args} {
    # Parse positional arguments
    set input [lindex $args 0]
    set option [expr {[llength $args] > 1 ? [lindex $args 1] : "default"}]

    # Your tool logic here
    # ...

    # Return result
    return $result
}
```

### Parameter Definition

Each parameter in the `info` dict must specify:
- `type`: Parameter type (string, int, bool, list, dict)
- `required`: Whether the parameter is required (true/false)
- `description`: Human-readable description

### Return Value Types

The return value type determines how the output is formatted:

| Type | Output Format |
|------|---------------|
| `string` | Raw content (no JSON encoding) |
| `list` | JSON array |
| `dict` | JSON object |
| `int`, `bool` | Raw value |

**Example:**
```tcl
# Returns raw string content
return "Hello World"

# Returns JSON array
return [list "item1" "item2" "item3"]

# Returns JSON object
return [dict create key1 "value1" key2 "value2"]
```

## Output Handling

### String Return Values

Tools that return strings have their output returned as-is, without JSON encoding or escaping:

```tcl
proc execute {args} {
    set fp [open [lindex $args 0] r]
    set content [read $fp]
    close $fp
    return $content
}
```

This preserves newlines, special characters, and formatting.

### List/Dict Return Values

Tools that return lists or dicts have their output JSON-encoded:

```tcl
proc execute {args} {
    return [list "item1" "item2" "item3"]
}
# Output: ["item1", "item2", "item3"]
```

## Implementation Details

### Parameter Parsing

The CLI uses named parameters (`--key value`) which are automatically converted to positional arguments for the TCL `execute` procedure:

```bash
# CLI command
emx-llm tools mytool --input "value" --option "value"

# Becomes (in TCL)
execute "value" "value"
```

### Type Detection

The system uses `Value::type_name()` to detect the return type and choose the appropriate output format:

```rust
match result.type_name() {
    "dict" | "list" => {
        // Use JSON encoding
        interp.eval("json::encode $_tool_result list")
    }
    _ => {
        // Return raw string content
        result.as_str().to_string()
    }
}
```

### Tool Discovery

Tools are automatically discovered from `.tcl` files in the `tools/` directory. The tool name is derived from the filename (without the `.tcl` extension).

## Environment Variables

- `EMX_TOOLS_DIR`: Custom tools directory (default: `./tools`)

## Examples

### Example 1: Find all Rust source files

```bash
emx-llm tools glob --pattern "**/*.rs"
```

### Example 2: Read configuration file

```bash
emx-llm tools read --path Cargo.toml | head -10
```

### Example 3: Chain tools with other commands

```bash
# Find all .rs files and count them
emx-llm tools glob --pattern "**/*.rs" | wc -l

# Find files and read each one
emx-llm tools glob --pattern "*.md" | while read file; do
    echo "=== $file ==="
    cat "$file"
done
```

## Error Handling

- **Tool not found**: Returns error if tool script doesn't exist
- **Missing required parameter**: Validates required parameters before calling tool
- **TCL errors**: TCL script errors are propagated with context

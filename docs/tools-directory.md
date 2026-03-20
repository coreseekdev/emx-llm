# Tools 目录

本目录包含 emx-llm 工具调用系统的工具定义，每个工具对应一个 TCL 脚本文件。

## 目录结构

```
tools/
├── README.md                # 本文档
├── read.tcl                 # 默认工具：读取文件内容
└── glob.tcl                 # 默认工具：文件路径匹配
```

## 默认工具集

emx-llm 默认提供两个基础工具：

| 工具 | 功能 |
|------|------|
| **read** | 读取文件内容 |
| **glob** | 文件路径匹配（通配符搜索） |

其他工具由用户根据需要自行添加到 tools/ 目录。

## 工具脚本格式

每个 TCL 工具脚本必须实现两个命令：

### 1. info 命令（必填）

返回工具的元数据信息，格式为 Tcl 字典：

```tcl
proc info {} {
    return [dict create \
        name "read" \
        description "Read the contents of a file" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The file path to read" \
            ] \
        ] \
        returns "The file contents as a string" \
        example "read /path/to/file.txt"
    ]
}
```

### 2. execute 命令（必填）

执行工具的实际逻辑：

```tcl
proc execute {args} {
    set path [lindex $args 0]

    if {$path eq ""} {
        error "Missing required parameter: path"
    }

    set fp [open $path r]
    set content [read $fp]
    close $fp

    return [dict create content $content]
}
```

## 元数据字典格式

### info 返回的顶级字典

| 键 | 类型 | 必填 | 说明 |
|---|------|------|------|
| `name` | string | ✅ | 工具名称 |
| `description` | string | ✅ | 工具描述 |
| `parameters` | dict | ✅ | 参数字典 |
| `returns` | string | ❌ | 返回值描述 |
| `example` | string | ❌ | 使用示例 |

### 参数字典结构

每个参数都是一个嵌套字典，包含：

| 键 | 类型 | 必填 | 说明 |
|---|------|------|------|
| `type` | string | ✅ | 参数类型 |
| `required` | boolean | ✅ | 是否必填 |
| `description` | string | ✅ | 参数描述 |

## 参数类型

| 类型 | 说明 | 示例值 |
|------|------|--------|
| `string` | 字符串 | `"file.txt"` |
| `integer` | 整数 | `42` |
| `number` | 浮点数 | `3.14` |
| `boolean` | 布尔值 | `true` / `false` |
| `array` | 数组 | `["a", "b"]` |
| `object` | 对象 | `{"key": "value"}` |

## 简单工具模板

```tcl
# 工具信息
proc info {} {
    return [dict create \
        name "my_tool" \
        description "Description of what this tool does" \
        parameters [dict create \
            input [dict create \
                type "string" \
                required true \
                description "The input parameter" \
            ] \
        ] \
    ]
}

# 工具实现
proc execute {args} {
    set input [lindex $args 0]

    if {$input eq ""} {
        error "Missing required parameter: input"
    }

    # 处理逻辑...

    return [dict create result "success"]
}
```

## 辅助函数

### 参数解析

```tcl
proc parse_params {args spec} {
    # args: 命令行参数列表
    # spec: 参数规格列表
    #   格式: {{param_name default_value required} ...}

    array set result {}
    set arg_idx 0

    foreach param_spec $spec {
        set param_name [lindex $param_spec 0]
        set default_val [lindex $param_spec 1]
        set required [lindex $param_spec 2]

        if {$arg_idx < [llength $args]} {
            set result($param_name) [lindex $args $arg_idx]
            incr arg_idx
        } else {
            if {$required} {
                error "Missing required parameter: $param_name"
            }
            set result($param_name) $default_val
        }
    }

    return [array get result]
}
```

### JSON 输出

```tcl
proc json_write {dict_val} {
    # 简化的 JSON 生成
    # 对于简单的键值对:
    set json "{"
    set first 1
    dict for {key value} $dict_val {
        if {!$first} {
            append json ","
        }
        append json "\"$key\":"

        # 判断值类型
        if {[string is integer $value]} {
            append json "$value"
        } elseif {[string is boolean $value]} {
            append json [expr {$value ? "true" : "false"}]
        } else {
            # 字符串需要转义
            set escaped [string map {"\\" "\\\\"} $value]
            append json "\"$escaped\""
        }

        set first 0
    }
    append json "}"

    return $json
}
```

### 路径验证

```tcl
proc validate_path {path {base_dir "."}} {
    # 检查路径是否在允许的目录内
    set abs_path [file normalize $path]
    set abs_base [file normalize $base_dir]

    if {![string match "${abs_base}*" $abs_path]} {
        error "Path outside allowed directory: $path"
    }

    return $abs_path
}
```

## 最佳实践

### 1. 错误处理

```tcl
proc tool_main {args} {
    # 使用 catch 捕获错误
    if {[catch {
        # 工具逻辑
        set fp [open $file_path "r"]
        set content [read $fp]
        close $fp
    } err]} {
        # 返回错误信息
        return [json_write {
            success false
            error $err
        }]
    }

    return [json_write {
        success true
        content $content
    }]
}
```

### 2. 输入验证

```tcl
proc tool_main {args} {
    array set params [parse_params $args {
        {path "" true}
    }]

    # 验证文件存在
    if {![file exists $params(path)]} {
        error "File not found: $params(path)"
    }

    # 验证文件可读
    if {[file readable $params(path)] == 0} {
        error "File not readable: $params(path)"
    }

    # 继续处理...
}
```

### 3. 资源清理

```tcl
proc tool_main {args} {
    set fp ""

    if {[catch {
        set fp [open $file_path "r"]
        # 处理文件...
        close $fp
    } err]} {
        if {$fp ne ""} {
            catch {close $fp}
        }
        error $err
    }
}
```

### 4. 返回值格式

推荐使用一致的 JSON 返回格式：

```tcl
# 成功时
{
    "success": true,
    "result": ...,
    "message": "操作成功"
}

# 失败时
{
    "success": false,
    "error": "错误描述",
    "code": "ERROR_CODE"
}
```

## 工具开发流程

1. **创建工具脚本**
   ```bash
   # 在 tools/ 目录下创建新的 .tcl 文件
   touch tools/my_tool.tcl
   ```

2. **编写元数据和实现**
   ```tcl
   # 参考上面的模板编写工具
   ```

3. **本地测试**
   ```bash
   # 使用 rtcl CLI 测试工具
   rtcl -f tools/my_tool.tcl -c "tool_main arg1 arg2"
   ```

4. **验证工具定义**
   ```bash
   # 验证元数据是否正确
   emx-llm tools validate my_tool
   ```

5. **集成测试**
   ```bash
   # 在实际对话中测试
   emx-llm chat test-session --tools my_tool "使用我的工具"
   ```

## 常见问题

### Q: 如何返回复杂的数据结构？

A: 使用嵌套的字典或列表，确保 JSON 格式正确：

```tcl
return [json_write {
    files [list \
        [dict create path "a.txt" size 100] \
        [dict create path "b.txt" size 200]
    ]
}]
```

### Q: 如何处理二进制文件？

A: 对二进制文件使用 base64 编码：

```tcl
# 读取二进制文件
set fp [open $binary_file "rb"]
set binary_data [read $fp]
close $fp

# Base64 编码（需要内置函数或 Tcl 扩展）
set encoded [base64::encode $binary_data]

return [json_write {
    content $encoded
    encoding "base64"
}]
```

### Q: 工具可以调用系统命令吗？

A: 为了安全考虑，默认禁止执行系统命令。如需此功能，需要在工具管理器中显式启用。

## 参考资料

- [Tcl 语法文档](https://www.tcl.tk/man/tcl/TclLib/Tcl.htm)
- [emx-llm Tool Call 设计](../docs/tool-call-design.md)
- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling)
- [Anthropic Tool Use](https://docs.anthropic.com/claude/docs/tool-use)
- [rtcl 文档](https://github.com/rtcl-project/rtcl)

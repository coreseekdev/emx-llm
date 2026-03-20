# 工具脚本模板

本文档提供工具脚本模板，基于 `info`/`execute` 命令机制。

## 默认工具集

### read.tcl - 读取文件内容

```tcl
# tools/read.tcl
# 读取文件内容

# 工具信息
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

# 工具实现
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

### glob.tcl - 文件路径匹配

```tcl
# tools/glob.tcl
# 文件路径匹配（通配符搜索）

# 工具信息
proc info {} {
    return [dict create \
        name "glob" \
        description "Find files matching a pattern" \
        parameters [dict create \
            pattern [dict create \
                type "string" \
                required true \
                description "The glob pattern (e.g., *.rs, **/*.txt)" \
            ] \
            path [dict create \
                type "string" \
                required false \
                description "Base directory to search (default: current)" \
            ] \
        ] \
        returns "List of matching file paths" \
        example "glob **/*.md ."
}

# 工具实现
proc execute {args} {
    set pattern [lindex $args 0]
    set path [expr {[llength $args] > 1 ? [lindex $args 1] : "."}]

    if {$pattern eq ""} {
        error "Missing required parameter: pattern"
    }

    set matches [glob -nocomplain -directory $path -- $pattern]

    return [dict create matches $matches]
}
```

## 扩展工具模板

用户可以添加到 tools/ 目录的工具示例：

### 1. 写文件工具 (write.tcl)

```tcl
# 工具信息
proc info {} {
    return [dict create \
        name "write" \
        description "Write content to a file" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The file path to write" \
            ] \
            content [dict create \
                type "string" \
                required true \
                description "The content to write" \
            ] \
        ] \
    ]
}

# 工具实现
proc execute {args} {
    set path [lindex $args 0]
    set content [lindex $args 1]

    if {$path eq ""} {
        error "Missing required parameter: path"
    }

    set fp [open $path w]
    puts -nonewline $fp $content
    close $fp

    return [dict create \
        success true \
        message "File written successfully" \
    ]
}
```

### 2. 列出目录工具 (list_files.tcl)

```tcl
# 工具信息
proc info {} {
    return [dict create \
        name "write" \
        description "Write content to a file" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The file path to write" \
            ] \
            content [dict create \
                type "string" \
                required true \
                description "The content to write" \
            ] \
        ] \
    ]
}

# 工具实现
proc execute {args} {
    set path [lindex $args 0]
    set content [lindex $args 1]

    if {$path eq ""} {
        error "Missing required parameter: path"
    }

    set fp [open $path w]
    puts -nonewline $fp $content
    close $fp

    return [dict create \
        success true \
        message "File written successfully" \
    ]
}
```

### 2. 文件匹配工具 (glob.tcl)

```tcl
proc info {} {
    return [dict create \
        name "glob" \
        description "Find files matching a pattern" \
        parameters [dict create \
            pattern [dict create \
                type "string" \
                required true \
                description "The glob pattern (e.g., *.rs)" \
            ] \
            path [dict create \
                type "string" \
                required false \
                description "Base directory (default: current)" \
            ] \
        ] \
    ]
}

proc execute {args} {
    set pattern [lindex $args 0]
    set path [expr {[llength $args] > 1 ? [lindex $args 1] : "."}]

    set matches [glob -nocomplain -directory $path -- $pattern]

    return [dict create matches $matches]
}
```

### 3. 列出目录工具 (list_files.tcl)

```tcl
proc info {} {
    return [dict create \
        name "list_files" \
        description "List files and directories" \
        parameters [dict create \
            path [dict create \
                type "string" \
                required true \
                description "The directory path" \
            ] \
        ] \
    ]
}

proc execute {args} {
    set path [lindex $args 0]

    set entries [glob -nocomplain -directory $path -- *]
    set result {}

    foreach entry $entries {
        file stat $entry stat
        set type [file isfile $entry ? "file" : "directory"]

        lappend result [dict create \
            path $entry \
            type $type \
            size $stat(size) \
        ]
    }

    return [dict create entries $result]
}
```

## 辅助函数

### dict_to_json 转换

```tcl
proc dict_to_json {dict_val} {
    set json "{"
    set first 1

    dict for {key value} $dict_val {
        if {!$first} {
            append json ","
        }
        append json "\"$key\":"

        # 类型判断和转换
        if {[string is integer $value]} {
            append json $value
        } elseif {$value eq "true"} {
            append json "true"
        } elseif {$value eq "false"} {
            append json "false"
        } else {
            # 字符串
            append json "\"$value\""
        }

        set first 0
    }

    append json "}"
    return $json
}
```

## 调试技巧

### 测试工具元数据

```tcl
# 在 TCL REPL 中测试
package require rtcl
source tools/read.tcl

# 查看工具信息
info
# => name read description {...} parameters {...}

# 测试执行
execute /path/to/file.txt
```

### 验证 JSON 输出

```tcl
proc execute {args} {
    # ... 工具逻辑 ...

    set result [dict create content $content]

    # 调试：打印字典
    if {[info exists env(TOOL_DEBUG)]} {
        puts stderr "DEBUG: [dict get $result content]"
    }

    return $result
}
```

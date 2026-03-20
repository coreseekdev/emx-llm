# tools/glob.tcl
# 文件路径匹配（通配符搜索）工具

# 工具信息
proc info {} {
    return [dict create \
        name glob \
        description {Find files matching a pattern} \
        parameters [dict create \
            pattern [dict create \
                type string \
                required true \
                description {The glob pattern (e.g., *.rs, **/*.txt)} \
            ] \
            path [dict create \
                type string \
                required false \
                description {Base directory to search (default: current directory)} \
            ] \
        ] \
        returns {List of matching file paths} \
        example {glob **/*.md .} \
    ]
}

# 工具实现
proc execute {args} {
    set pattern [lindex $args 0]
    set path [expr {[llength $args] > 1 ? [lindex $args 1] : "."}]

    if {$pattern eq ""} {
        error "Missing required parameter: pattern"
    }

    return [glob -nocomplain -directory $path -- $pattern]
}

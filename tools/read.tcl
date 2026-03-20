# tools/read.tcl
# 读取文件内容工具

# 工具信息
proc info {} {
    return [dict create \
        name read \
        description {Read the contents of a file} \
        parameters [dict create \
            path [dict create \
                type string \
                required true \
                description {The file path to read} \
            ] \
        ] \
        returns {The file contents as a string} \
        example {read /path/to/file.txt} \
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

use emx_llm::load_tools_from_dir;
use std::path::Path;

fn main() {
    let tools = load_tools_from_dir(Some(Path::new("./tools"))).unwrap();
    println!("Loaded {} tools:", tools.len());
    for tool in &tools {
        println!("- {}", tool.name);
        let json = serde_json::to_string_pretty(&tool.to_openai()).unwrap();
        println!("{}", json);
    }
}

//! Test binary to capture raw upstream stream response

use emx_llm::{create_client_for_model, Message, MessageRole};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, model_id) = create_client_for_model("anthropic.glm-5")?;

    let messages = vec![
        Message::user("Say hi in 3 words")
    ];

    println!("=== Calling upstream API (stream mode) ===");
    let response = client.chat_stream_raw(&messages, &model_id).await?;

    println!("\nStatus: {}", response.status());
    println!("Headers:");
    for (name, value) in response.headers().iter() {
        println!("  {}: {}", name, value.to_str().unwrap_or(""));
    }

    println!("\n=== Raw Stream Response ===");
    let mut stream = response.bytes_stream();
    let mut full_output = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let chunk_str = String::from_utf8_lossy(&chunk);
        full_output.push_str(&chunk_str);
        print!("{}", chunk_str);
    }

    println!("\n\n=== Total bytes: {} ===", full_output.len());

    Ok(())
}

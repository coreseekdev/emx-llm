//! HTTP fixture recording and playback for testing
//!
//! This module provides functionality to record HTTP interactions to txtar format
//! and replay them later, enabling offline testing without real API calls.
//!
//! Uses the `emx-txtar` crate for proper txtar encoding/decoding instead of
//! hand-rolling the format.

use std::path::Path;

use anyhow::Result;
use emx_txtar::{Archive, Decoder, Encoder, File as TxtarFile};

/// HTTP fixture recorder that saves responses to txtar format
pub struct FixtureRecorder {
    fixtures: Vec<(String, String)>, // (name, content)
}

impl FixtureRecorder {
    /// Create a new fixture recorder
    pub fn new() -> Self {
        Self {
            fixtures: Vec::new(),
        }
    }

    /// Record a response
    pub fn record(&mut self, name: impl AsRef<str>, content: impl AsRef<str>) {
        self.fixtures
            .push((name.as_ref().to_string(), content.as_ref().to_string()));
    }

    /// Write all recorded fixtures to a txtar file
    pub fn write_to_txtar<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut archive = Archive::new();
        for (name, content) in &self.fixtures {
            archive.add_file(TxtarFile::new(name.clone(), content.as_bytes().to_vec()))?;
        }

        let encoder = Encoder::new();
        encoder.encode_to_file(&archive, path)?;

        Ok(())
    }

    /// Load fixtures from a txtar file
    pub fn load_from_txtar<P: AsRef<Path>>(path: P) -> Result<Vec<(String, String)>> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;

        let decoder = Decoder::new();
        let archive = decoder.decode(&content)?;

        let mut fixtures = Vec::new();
        for file in &archive.files {
            let name = file.name.clone();
            let text = String::from_utf8(file.data.clone())
                .map_err(|e| anyhow::anyhow!("fixture '{}' is not valid UTF-8: {}", name, e))?;
            fixtures.push((name, text));
        }

        Ok(fixtures)
    }
}

impl Default for FixtureRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Example fixture file structure for OpenAI chat completion
///
/// ```text
/// -- openai_chat_completion_success.json --
/// {
///   "id": "chatcmpl-mock",
///   "object": "chat.completion",
///   "created": 1234567890,
///   "model": "glm-4-flash",
///   "choices": [{
///     "index": 0,
///     "message": {
///       "role": "assistant",
///       "content": "Hello, world!"
///     },
///     "finish_reason": "stop"
///   }],
///   "usage": {
///     "prompt_tokens": 10,
///     "completion_tokens": 5,
///     "total_tokens": 15
///   }
/// }
///
/// -- openai_streaming_chunk.txt --
/// data: {"id":"chatcmpl-mock","object":"chat.completion.chunk",...}
///
/// data: {"id":"chatcmpl-mock","object":"chat.completion.chunk",...}
///
/// data: [DONE]
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_recorder_write_and_load() {
        let mut recorder = FixtureRecorder::new();
        recorder.record("test1.json", r#"{"key": "value"}"#);
        recorder.record("test2.txt", "Hello, world!");

        let temp_dir = std::env::temp_dir();
        let txtar_path = temp_dir.join("test_fixtures.txtar");

        recorder.write_to_txtar(&txtar_path).unwrap();
        let fixtures = FixtureRecorder::load_from_txtar(&txtar_path).unwrap();

        assert_eq!(fixtures.len(), 2);
        assert_eq!(fixtures[0].0, "test1.json");
        assert_eq!(fixtures[0].1, r#"{"key": "value"}"#);
        assert_eq!(fixtures[1].0, "test2.txt");
        assert_eq!(fixtures[1].1, "Hello, world!");

        std::fs::remove_file(&txtar_path).ok();
    }

    #[test]
    fn test_fixture_recorder_multiline_content() {
        let mut recorder = FixtureRecorder::new();
        recorder.record("multiline.txt", "Line 1\nLine 2\nLine 3");

        let temp_dir = std::env::temp_dir();
        let txtar_path = temp_dir.join("test_multiline.txtar");

        recorder.write_to_txtar(&txtar_path).unwrap();
        let fixtures = FixtureRecorder::load_from_txtar(&txtar_path).unwrap();

        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].1, "Line 1\nLine 2\nLine 3");

        std::fs::remove_file(&txtar_path).ok();
    }
}

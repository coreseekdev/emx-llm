use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use emx_mbox::{MailMessage, MailStore, Mbox, MessageBuilder};

use crate::{Message, MessageRole, Usage};

const SYSTEM_PREFIX: &str = "system";
const USER_PREFIX: &str = "user";
const TOOL_PREFIX: &str = "tool";
const DEFAULT_DOMAIN: &str = "emx-llm";

pub const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/system.md");

fn get_domain() -> String {
    std::env::var("EMX_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromInfo {
    System,
    User,
    Tool,
    Assistant { model: String },
    Agent { agent: String, model: String },
    Unknown,
}

pub fn role_from_mail(msg: &MailMessage) -> MessageRole {
    match parse_from_address(msg) {
        FromInfo::System => MessageRole::System,
        FromInfo::User => MessageRole::User,
        FromInfo::Tool
        | FromInfo::Assistant { .. }
        | FromInfo::Agent { .. }
        | FromInfo::Unknown => MessageRole::Assistant,
    }
}

pub fn parse_from_address(msg: &MailMessage) -> FromInfo {
    let from_value = msg
        .header("From")
        .or_else(|| msg.envelope_from())
        .unwrap_or_default();

    let address = extract_address(from_value);
    if address.is_empty() {
        return FromInfo::Unknown;
    }

    let local = address
        .split('@')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();

    let local_lower = local.to_lowercase();
    if local_lower == SYSTEM_PREFIX {
        return FromInfo::System;
    }
    if local_lower == USER_PREFIX {
        return FromInfo::User;
    }
    if local_lower == TOOL_PREFIX {
        return FromInfo::Tool;
    }

    if let Some((agent, model)) = local.split_once('#') {
        return FromInfo::Agent {
            agent: agent.to_string(),
            model: model.to_string(),
        };
    }

    FromInfo::Assistant { model: local }
}

fn extract_address(from_value: &str) -> String {
    let trimmed = from_value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let (Some(start), Some(end)) = (trimmed.find('<'), trimmed.rfind('>')) {
        if start < end {
            return trimmed[start + 1..end].trim().to_string();
        }
    }

    trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn message_content_from_mail(msg: &MailMessage) -> String {
    let mut content = msg.body().trim_end().to_string();
    for attachment in msg.attachments() {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&format!(
            "[Attachment: {}]\n{}",
            attachment.filename,
            String::from_utf8_lossy(&attachment.data)
        ));
    }
    content
}

fn enrich_user_content(content: &str, attachments: &[PathBuf]) -> Result<String> {
    let mut merged = content.trim_end().to_string();

    for path in attachments {
        let raw = fs::read(path)?;
        let text = String::from_utf8_lossy(&raw);
        if !merged.is_empty() {
            merged.push_str("\n\n");
        }
        merged.push_str(&format!(
            "[Attachment: {}]\n{}",
            path.file_name()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            text
        ));
    }

    Ok(merged)
}

fn build_user_mail(content: &str, attachments: &[PathBuf], domain: &str) -> Result<MailMessage> {
    let mut builder = MessageBuilder::new(format!("{}@{}", USER_PREFIX, domain), "").body(content.to_string());
    for attachment in attachments {
        builder = builder.attach_file(attachment)?;
    }
    Ok(builder.build())
}

pub struct Session {
    name: String,
    path: PathBuf,
    history: Vec<Message>,
    system_prompt: Option<String>,
}

impl Session {
    pub fn open(name: &str) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(anyhow!("session name is required"));
        }
        if name.contains(['/', '\\']) {
            return Err(anyhow!("session name must not contain path separators"));
        }

        let session_dir = Self::get_session_dir();
        fs::create_dir_all(&session_dir)?;

        let path = session_dir.join(format!("{}.mbox", name));
        let history = Self::load_history(&path)?;
        let system_prompt = history
            .iter()
            .find(|msg| msg.role == MessageRole::System)
            .map(|msg| msg.content.clone());

        Ok(Self {
            name: name.to_string(),
            path,
            history,
            system_prompt,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get_session_dir() -> PathBuf {
        if let Ok(custom) = std::env::var("EMX_SESSION_DIR") {
            return PathBuf::from(custom);
        }

        if let Some(home) = dirs::home_dir() {
            return home
                .join(".local")
                .join("share")
                .join("emx-llm")
                .join("sessions");
        }

        PathBuf::from(".emx-llm").join("sessions")
    }

    fn load_history(path: &Path) -> Result<Vec<Message>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let mbox = Mbox::load_file(path)?;
        let messages = mbox
            .messages()
            .iter()
            .map(|mail| Message {
                role: role_from_mail(mail),
                content: message_content_from_mail(mail),
            })
            .collect();

        Ok(messages)
    }

    pub fn validate_system_prompt(&self, provided: Option<&str>) -> Result<()> {
        if let (Some(existing), Some(incoming)) = (&self.system_prompt, provided) {
            if existing.trim() != incoming.trim() {
                return Err(anyhow!(
                    "system prompt mismatch for session '{}': existing prompt differs from --system",
                    self.name
                ));
            }
        }
        Ok(())
    }

    pub fn ensure_system_prompt(&mut self, provided: Option<&str>) -> Result<()> {
        self.validate_system_prompt(provided)?;

        if self.system_prompt.is_none() {
            let content = provided
                .map(|v| v.to_string())
                .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());
            let system_message = Message::system(content.clone());
            self.append(&system_message, None, None, None)?;
            self.history.push(system_message);
            self.system_prompt = Some(content);
        }

        Ok(())
    }

    pub fn append(
        &self,
        msg: &Message,
        model: Option<&str>,
        usage: Option<&Usage>,
        duration_ms: Option<u128>,
    ) -> Result<()> {
        let domain = get_domain();

        let mut builder = match msg.role {
            MessageRole::System => {
                MessageBuilder::new(format!("{}@{}", SYSTEM_PREFIX, domain), "").body(msg.content.clone())
            }
            MessageRole::User => {
                MessageBuilder::new(format!("{}@{}", USER_PREFIX, domain), "").body(msg.content.clone())
            }
            MessageRole::Assistant => {
                let model_name = model.unwrap_or("assistant");
                MessageBuilder::new(format!("{}@{}", model_name, domain), "").body(msg.content.clone())
            }
        };

        if let Some(usage) = usage {
            builder = builder.extra_header(
                "X-LLM-Tokens",
                format!(
                    "prompt={}; completion={}; total={}",
                    usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                ),
            );
        }

        if let Some(duration_ms) = duration_ms {
            builder = builder.extra_header("X-LLM-Duration-Ms", duration_ms.to_string());
        }

        let mail = builder.build();
        Mbox::append_to_file(&self.path, &mail)?;
        Ok(())
    }

    pub fn messages(&self) -> &[Message] {
        &self.history
    }

    pub fn preview_user_message(&self, content: String, attachments: &[PathBuf]) -> Result<Vec<Message>> {
        let enriched = enrich_user_content(&content, attachments)?;
        let mut messages = self.history.clone();
        messages.push(Message::user(enriched));
        Ok(messages)
    }

    pub fn add_user_message(&mut self, content: String, attachments: &[PathBuf]) -> Result<&[Message]> {
        let domain = get_domain();

        let mail = build_user_mail(&content, attachments, &domain)?;
        Mbox::append_to_file(&self.path, &mail)?;

        let enriched = enrich_user_content(&content, attachments)?;
        self.history.push(Message::user(enriched));
        Ok(&self.history)
    }

    pub fn add_assistant_response(
        &mut self,
        content: String,
        model: &str,
        usage: &Usage,
        duration_ms: Option<u128>,
    ) -> Result<()> {
        let message = Message::assistant(content);
        self.append(&message, Some(model), Some(usage), duration_ms)?;
        self.history.push(message);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock poisoned")
    }

    fn unique_session_dir() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("emx-llm-session-test-{}-{}", std::process::id(), ts))
    }

    #[test]
    fn assistant_headers_include_tokens_and_duration() {
        let _guard = env_lock();
        let dir = unique_session_dir();
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::env::set_var("EMX_SESSION_DIR", &dir);

        let mut session = Session::open("headers").expect("open session");
        session
            .ensure_system_prompt(Some("You are test system"))
            .expect("ensure system");
        session
            .add_user_message("hello".to_string(), &[])
            .expect("add user");
        let usage = Usage {
            prompt_tokens: 11,
            completion_tokens: 22,
            total_tokens: 33,
        };
        session
            .add_assistant_response("world".to_string(), "gpt-4", &usage, Some(3210))
            .expect("add assistant");

        let mbox = Mbox::load_file(dir.join("headers.mbox")).expect("load mbox");
        let last = mbox.messages().last().expect("has last message");

        assert_eq!(last.header("X-LLM-Tokens"), Some("prompt=11; completion=22; total=33"));
        assert_eq!(last.header("X-LLM-Duration-Ms"), Some("3210"));
        assert!(last.from().contains("gpt-4@"));
    }

    #[test]
    fn system_prompt_conflict_is_rejected() {
        let _guard = env_lock();
        let dir = unique_session_dir();
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::env::set_var("EMX_SESSION_DIR", &dir);

        let mut session = Session::open("prompt").expect("open session");
        session
            .ensure_system_prompt(Some("System A"))
            .expect("ensure system");

        let session2 = Session::open("prompt").expect("open existing session");
        let err = session2
            .validate_system_prompt(Some("System B"))
            .expect_err("must reject mismatch");
        assert!(err.to_string().contains("system prompt mismatch"));
    }

    #[test]
    fn preview_user_message_does_not_mutate_history() {
        let _guard = env_lock();
        let dir = unique_session_dir();
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::env::set_var("EMX_SESSION_DIR", &dir);

        let mut session = Session::open("dryrun").expect("open session");
        session
            .ensure_system_prompt(Some("System"))
            .expect("ensure system");

        let before = session.messages().len();
        let preview = session
            .preview_user_message("hello dry run".to_string(), &[])
            .expect("preview");

        assert_eq!(session.messages().len(), before);
        assert_eq!(preview.len(), before + 1);
    }
}
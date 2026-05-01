use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FrontendMessageKind {
    User,
    Assistant,
    System,
    Tool,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendMessage {
    pub content: String,
    pub kind: FrontendMessageKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SubmittedInput {
    Empty,
    SlashCommand(String),
    BangCommand(String),
    ChatMessage(String),
}

impl SubmittedInput {
    pub fn parse(input: &str) -> Self {
        if input.is_empty() {
            Self::Empty
        } else if input.starts_with('/') {
            Self::SlashCommand(input.to_string())
        } else if input.starts_with('!') {
            Self::BangCommand(input.to_string())
        } else {
            Self::ChatMessage(input.to_string())
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrontendSession {
    pub input: String,
    pub messages: Vec<FrontendMessage>,
    pub last_user_prompt: Option<String>,
    pub pending_request: bool,
    pub tool_iteration_count: u32,
    pub current_response: String,
}

impl FrontendSession {
    pub fn add_message(&mut self, content: impl Into<String>, kind: FrontendMessageKind) {
        self.messages.push(FrontendMessage {
            content: content.into(),
            kind,
        });
    }

    pub fn submit_input(&mut self) -> SubmittedInput {
        let input = std::mem::take(&mut self.input);
        let submitted = SubmittedInput::parse(&input);
        if let SubmittedInput::ChatMessage(message) = &submitted {
            self.last_user_prompt = Some(message.clone());
            self.pending_request = true;
            self.tool_iteration_count = 0;
        }
        submitted
    }

    pub fn start_assistant_request(&mut self) {
        self.pending_request = false;
        self.current_response.clear();
        self.add_message("...".to_string(), FrontendMessageKind::Assistant);
    }

    pub fn append_assistant_chunk(&mut self, chunk: &str) {
        self.current_response.push_str(chunk);
        if let Some(last) = self.messages.last_mut() {
            if last.kind == FrontendMessageKind::Assistant {
                last.content.clone_from(&self.current_response);
            }
        }
    }

    pub fn set_retry_status(&mut self, content: String) {
        if let Some(last) = self.messages.last_mut() {
            if last.kind == FrontendMessageKind::Assistant {
                last.content = content;
                return;
            }
        }
        self.add_message(content, FrontendMessageKind::Assistant);
    }

    pub fn finish_assistant_message(&mut self, content: String) {
        self.pending_request = false;
        self.current_response.clone_from(&content);
        if let Some(last) = self.messages.last_mut() {
            if last.kind == FrontendMessageKind::Assistant {
                last.content = content;
                return;
            }
        }
        self.add_message(content, FrontendMessageKind::Assistant);
    }

    pub fn finish_error(&mut self, content: String) {
        self.pending_request = false;
        self.current_response.clear();
        if let Some(last) = self.messages.last_mut() {
            if last.kind == FrontendMessageKind::Assistant {
                last.kind = FrontendMessageKind::Error;
                last.content = content;
                return;
            }
        }
        self.add_message(content, FrontendMessageKind::Error);
    }
}

pub trait RunController {
    fn start_request(&mut self);
    fn append_chunk(&mut self, chunk: &str);
    fn set_retry_status(&mut self, status: impl Into<String>);
    fn finish_success(&mut self, content: impl Into<String>);
    fn finish_error(&mut self, content: impl Into<String>);
}

pub struct SessionRunController<'a> {
    session: &'a mut FrontendSession,
}

impl<'a> SessionRunController<'a> {
    pub const fn new(session: &'a mut FrontendSession) -> Self {
        Self { session }
    }
}

impl RunController for SessionRunController<'_> {
    fn start_request(&mut self) {
        self.session.start_assistant_request();
    }

    fn append_chunk(&mut self, chunk: &str) {
        self.session.append_assistant_chunk(chunk);
    }

    fn set_retry_status(&mut self, status: impl Into<String>) {
        self.session.set_retry_status(status.into());
    }

    fn finish_success(&mut self, content: impl Into<String>) {
        self.session.finish_assistant_message(content.into());
    }

    fn finish_error(&mut self, content: impl Into<String>) {
        self.session.finish_error(content.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_special_commands() {
        assert_eq!(
            SubmittedInput::parse("/help"),
            SubmittedInput::SlashCommand("/help".to_string())
        );
        assert_eq!(
            SubmittedInput::parse("!ls"),
            SubmittedInput::BangCommand("!ls".to_string())
        );
        assert_eq!(
            SubmittedInput::parse("hello"),
            SubmittedInput::ChatMessage("hello".to_string())
        );
        assert_eq!(SubmittedInput::parse(""), SubmittedInput::Empty);
    }

    #[test]
    fn submit_input_records_chat_state() {
        let mut session = FrontendSession {
            input: "hello".to_string(),
            ..Default::default()
        };

        let submitted = session.submit_input();

        assert_eq!(submitted, SubmittedInput::ChatMessage("hello".to_string()));
        assert_eq!(session.input, "");
        assert!(session.messages.is_empty());
        assert_eq!(session.last_user_prompt.as_deref(), Some("hello"));
        assert!(session.pending_request);
        assert_eq!(session.tool_iteration_count, 0);
    }

    #[test]
    fn request_lifecycle_updates_assistant_message() {
        let mut session = FrontendSession::default();
        session.start_assistant_request();
        session.append_assistant_chunk("hel");
        session.append_assistant_chunk("lo");
        session.finish_assistant_message("hello".to_string());

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[0].content, "hello");
        assert_eq!(session.current_response, "hello");
        assert!(!session.pending_request);
    }

    #[test]
    fn finish_error_converts_assistant_to_error() {
        let mut session = FrontendSession::default();
        session.start_assistant_request();
        session.append_assistant_chunk("thinking...");
        session.finish_error("rate limited".to_string());

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Error);
        assert_eq!(session.messages[0].content, "rate limited");
        assert!(session.current_response.is_empty());
    }

    #[test]
    fn finish_error_without_prior_assistant() {
        let mut session = FrontendSession::default();
        session.finish_error("immediate error".to_string());

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Error);
        assert_eq!(session.messages[0].content, "immediate error");
    }

    #[test]
    fn set_retry_status_updates_last_assistant() {
        let mut session = FrontendSession::default();
        session.start_assistant_request();
        session.set_retry_status("retrying...".to_string());

        assert_eq!(session.messages[0].content, "retrying...");
    }

    #[test]
    fn add_message_appends_correctly() {
        let mut session = FrontendSession::default();
        session.add_message("hello", FrontendMessageKind::User);
        session.add_message("world", FrontendMessageKind::System);

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::System);
    }

    #[test]
    fn frontend_message_kind_serde_roundtrip() {
        for kind in &[
            FrontendMessageKind::User,
            FrontendMessageKind::Assistant,
            FrontendMessageKind::System,
            FrontendMessageKind::Tool,
            FrontendMessageKind::Error,
        ] {
            let json = serde_json::to_string(kind).unwrap();
            let decoded: FrontendMessageKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, decoded);
        }
    }

    #[test]
    fn submit_input_slash_command_does_not_set_pending() {
        let mut session = FrontendSession {
            input: "/help".to_string(),
            ..Default::default()
        };
        let submitted = session.submit_input();
        assert!(matches!(submitted, SubmittedInput::SlashCommand(_)));
        assert!(!session.pending_request);
        assert!(session.last_user_prompt.is_none());
    }

    #[test]
    fn session_controller_lifecycle() {
        let mut session = FrontendSession::default();
        {
            let mut ctrl = SessionRunController::new(&mut session);
            ctrl.start_request();
            ctrl.append_chunk("hi");
            ctrl.finish_success("hi there");
        }
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "hi there");
    }

    #[test]
    fn frontend_message_serde_roundtrip() {
        let msg = FrontendMessage {
            content: "hello world".to_string(),
            kind: FrontendMessageKind::User,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: FrontendMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.content, "hello world");
        assert_eq!(decoded.kind, FrontendMessageKind::User);
    }

    #[test]
    fn frontend_message_kind_all_variants_serde() {
        let variants = vec![
            FrontendMessageKind::User,
            FrontendMessageKind::Assistant,
            FrontendMessageKind::System,
            FrontendMessageKind::Tool,
            FrontendMessageKind::Error,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let decoded: FrontendMessageKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, decoded);
            assert!(!json.contains('{'));
        }
    }

    #[test]
    fn frontend_session_default() {
        let session = FrontendSession::default();
        assert!(session.input.is_empty());
        assert!(session.messages.is_empty());
        assert!(session.last_user_prompt.is_none());
        assert!(!session.pending_request);
        assert_eq!(session.tool_iteration_count, 0);
        assert!(session.current_response.is_empty());
    }

    #[test]
    fn frontend_session_serde_roundtrip() {
        let mut session = FrontendSession {
            input: "test".to_string(),
            pending_request: true,
            last_user_prompt: Some("test".to_string()),
            ..Default::default()
        };
        session.add_message("user msg", FrontendMessageKind::User);
        session.add_message("assistant msg", FrontendMessageKind::Assistant);

        let json = serde_json::to_string(&session).unwrap();
        let decoded: FrontendSession = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.input, "test");
        assert_eq!(decoded.messages.len(), 2);
        assert_eq!(decoded.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(decoded.messages[1].kind, FrontendMessageKind::Assistant);
        assert!(decoded.pending_request);
        assert_eq!(decoded.last_user_prompt.as_deref(), Some("test"));
    }

    #[test]
    fn submitted_input_parse_whitespace_only_is_chat_message() {
        let result = SubmittedInput::parse("   ");
        assert!(matches!(result, SubmittedInput::ChatMessage(_)));
    }

    #[test]
    fn submitted_input_parse_slash_only() {
        let result = SubmittedInput::parse("/");
        assert!(matches!(result, SubmittedInput::SlashCommand(s) if s == "/"));
    }

    #[test]
    fn submitted_input_parse_bang_only() {
        let result = SubmittedInput::parse("!");
        assert!(matches!(result, SubmittedInput::BangCommand(s) if s == "!"));
    }

    #[test]
    fn submitted_input_parse_slash_with_args() {
        let result = SubmittedInput::parse("/model gpt-4");
        assert!(matches!(result, SubmittedInput::SlashCommand(s) if s == "/model gpt-4"));
    }

    #[test]
    fn submitted_input_parse_bang_with_args() {
        let result = SubmittedInput::parse("!cargo test");
        assert!(matches!(result, SubmittedInput::BangCommand(s) if s == "!cargo test"));
    }

    #[test]
    fn submit_input_empty() {
        let mut session = FrontendSession {
            input: String::new(),
            ..Default::default()
        };
        let result = session.submit_input();
        assert!(matches!(result, SubmittedInput::Empty));
        assert!(!session.pending_request);
        assert!(session.last_user_prompt.is_none());
    }

    #[test]
    fn submit_input_bang_command_does_not_set_pending() {
        let mut session = FrontendSession {
            input: "!ls".to_string(),
            ..Default::default()
        };
        let result = session.submit_input();
        assert!(matches!(result, SubmittedInput::BangCommand(_)));
        assert!(!session.pending_request);
        assert!(session.last_user_prompt.is_none());
    }

    #[test]
    fn submit_input_clears_input_field() {
        let mut session = FrontendSession {
            input: "hello".to_string(),
            ..Default::default()
        };
        session.submit_input();
        assert!(session.input.is_empty());
    }

    #[test]
    fn submit_input_resets_tool_iteration_count() {
        let mut session = FrontendSession {
            input: "hello".to_string(),
            ..Default::default()
        };
        session.tool_iteration_count = 5;
        session.submit_input();
        assert_eq!(session.tool_iteration_count, 0);
    }

    #[test]
    fn append_assistant_chunk_without_prior_assistant() {
        let mut session = FrontendSession::default();
        session.append_assistant_chunk("hello");
        assert!(session.current_response.contains("hello"));
        assert!(session.messages.is_empty());
    }

    #[test]
    fn append_assistant_chunk_accumulates() {
        let mut session = FrontendSession::default();
        session.start_assistant_request();
        session.append_assistant_chunk("hel");
        session.append_assistant_chunk("lo ");
        session.append_assistant_chunk("world");
        assert_eq!(session.current_response, "hello world");
        assert_eq!(session.messages[0].content, "hello world");
    }

    #[test]
    fn set_retry_status_no_assistant_adds_message() {
        let mut session = FrontendSession::default();
        session.set_retry_status("retrying...".to_string());
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[0].content, "retrying...");
    }

    #[test]
    fn set_retry_status_does_not_affect_non_assistant_last() {
        let mut session = FrontendSession::default();
        session.add_message("user msg", FrontendMessageKind::User);
        session.set_retry_status("retrying...".to_string());
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[1].content, "retrying...");
    }

    #[test]
    fn finish_assistant_message_no_prior_assistant() {
        let mut session = FrontendSession::default();
        session.finish_assistant_message("result".to_string());
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "result");
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Assistant);
        assert!(!session.pending_request);
    }

    #[test]
    fn finish_assistant_message_with_user_last() {
        let mut session = FrontendSession::default();
        session.add_message("user", FrontendMessageKind::User);
        session.finish_assistant_message("response".to_string());
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[1].content, "response");
    }

    #[test]
    fn finish_error_with_user_last() {
        let mut session = FrontendSession::default();
        session.add_message("user input", FrontendMessageKind::User);
        session.finish_error("something broke".to_string());
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Error);
        assert_eq!(session.messages[1].content, "something broke");
    }

    #[test]
    fn finish_error_clears_current_response() {
        let mut session = FrontendSession::default();
        session.start_assistant_request();
        session.append_assistant_chunk("partial...");
        assert!(!session.current_response.is_empty());
        session.finish_error("error!".to_string());
        assert!(session.current_response.is_empty());
    }

    #[test]
    fn session_controller_error_lifecycle() {
        let mut session = FrontendSession::default();
        {
            let mut ctrl = SessionRunController::new(&mut session);
            ctrl.start_request();
            ctrl.append_chunk("partial");
            ctrl.finish_error("rate limited");
        }
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Error);
        assert_eq!(session.messages[0].content, "rate limited");
        assert!(!session.pending_request);
    }

    #[test]
    fn session_controller_retry_lifecycle() {
        let mut session = FrontendSession::default();
        {
            let mut ctrl = SessionRunController::new(&mut session);
            ctrl.start_request();
            ctrl.append_chunk("partial...");
            ctrl.set_retry_status("Retrying...");
        }
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "Retrying...");
    }

    #[test]
    fn add_message_accepts_str_ref() {
        let mut session = FrontendSession::default();
        let msg: &str = "hello";
        session.add_message(msg, FrontendMessageKind::System);
        assert_eq!(session.messages[0].content, "hello");
    }

    #[test]
    fn start_assistant_request_clears_pending_and_response() {
        let mut session = FrontendSession {
            pending_request: true,
            current_response: "old response".to_string(),
            ..Default::default()
        };
        session.start_assistant_request();
        assert!(!session.pending_request);
        assert!(session.current_response.is_empty());
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "...");
        assert_eq!(session.messages[0].kind, FrontendMessageKind::Assistant);
    }

    #[test]
    fn multiple_messages_and_lifecycle() {
        let mut session = FrontendSession::default();
        session.add_message("system prompt", FrontendMessageKind::System);
        session.input = "user question".to_string();
        let submitted = session.submit_input();
        assert!(matches!(submitted, SubmittedInput::ChatMessage(_)));
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.last_user_prompt.as_deref(), Some("user question"));

        session.start_assistant_request();
        session.append_assistant_chunk("ans");
        session.append_assistant_chunk("wer");
        session.finish_assistant_message("answer".to_string());

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::System);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[1].content, "answer");
    }
}

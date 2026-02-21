use crate::model::types::{Message, MessageRole, SessionMeta};

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
}

impl AgentSession {
    pub fn new(meta: SessionMeta) -> Self {
        Self {
            meta,
            messages: Vec::new(),
        }
    }

    pub fn append_message(
        &mut self,
        id: impl Into<String>,
        role: MessageRole,
        content: impl Into<String>,
        timestamp: u64,
    ) {
        self.messages.push(Message {
            id: id.into(),
            role,
            content: content.into(),
            timestamp,
        });
        self.meta.updated_at = timestamp;
    }
}

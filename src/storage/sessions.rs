use crate::model::types::{Message, ProviderEvent, SessionMeta, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionEvent {
    Message(Message),
    Provider(ProviderEvent),
    ToolResult(ToolResult),
    Status(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub meta: SessionMeta,
    pub events: Vec<SessionEvent>,
}

pub trait SessionStore: Send + Sync {
    fn append_event(
        &self,
        session_id: &str,
        model: &str,
        event: &SessionEvent,
    ) -> anyhow::Result<()>;

    fn load_session(&self, session_id: &str) -> anyhow::Result<Option<SessionRecord>>;

    fn load_latest(&self) -> anyhow::Result<Option<SessionRecord>>;
}

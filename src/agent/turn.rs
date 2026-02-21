use crate::model::types::ProviderEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRequest {
    pub session_id: String,
    pub model: String,
    pub user_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    pub events: Vec<ProviderEvent>,
}

impl TurnOutcome {
    pub fn completed_with_text(text: impl Into<String>) -> Self {
        Self {
            events: vec![
                ProviderEvent::TextDelta(text.into()),
                ProviderEvent::Completed,
            ],
        }
    }
}

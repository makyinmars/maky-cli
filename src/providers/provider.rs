use async_trait::async_trait;

use crate::model::types::{Message, ProviderEvent};

#[derive(Debug, Clone)]
pub struct ProviderTurnRequest {
    pub session_id: String,
    pub model: String,
    pub messages: Vec<Message>,
}

pub type ProviderEventStream = Vec<ProviderEvent>;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn stream_turn(
        &self,
        request: ProviderTurnRequest,
    ) -> anyhow::Result<ProviderEventStream>;
}

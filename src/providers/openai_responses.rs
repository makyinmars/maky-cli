use anyhow::bail;
use async_trait::async_trait;

use crate::{
    model::types::ProviderEvent,
    providers::provider::{ModelProvider, ProviderEventStream, ProviderTurnRequest},
};

#[derive(Debug, Default)]
pub struct OpenAiResponsesProvider;

#[async_trait]
impl ModelProvider for OpenAiResponsesProvider {
    fn id(&self) -> &'static str {
        "openai-responses"
    }

    async fn stream_turn(
        &self,
        request: ProviderTurnRequest,
    ) -> anyhow::Result<ProviderEventStream> {
        if request.messages.is_empty() {
            bail!("provider request must include at least one message")
        }

        let summary = format!(
            "stubbed provider for model {} (session {})",
            request.model, request.session_id
        );

        Ok(vec![
            ProviderEvent::TextDelta(summary),
            ProviderEvent::Completed,
        ])
    }
}

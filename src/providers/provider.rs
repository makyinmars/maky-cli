use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};

use async_trait::async_trait;

use crate::model::types::{Message, ProviderEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAuthContext {
    pub access_token: Option<String>,
    pub source: String,
    pub status: String,
    pub provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTurnRequest {
    pub turn_id: String,
    pub session_id: String,
    pub model: String,
    pub variant: String,
    pub messages: Vec<Message>,
    pub auth: ProviderAuthContext,
}

pub type ProviderEventReceiver = Receiver<ProviderEvent>;

#[derive(Debug, Clone)]
pub struct TurnHandle {
    turn_id: String,
    cancellation_flag: Arc<AtomicBool>,
}

impl TurnHandle {
    pub fn new(turn_id: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            cancellation_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn cancel(&self) -> bool {
        self.cancellation_flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation_flag.load(Ordering::SeqCst)
    }

    pub fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancellation_flag)
    }
}

pub struct ProviderTurn {
    pub turn_id: String,
    pub event_rx: ProviderEventReceiver,
    pub handle: TurnHandle,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn stream_turn(&self, request: ProviderTurnRequest) -> anyhow::Result<ProviderTurn>;
}

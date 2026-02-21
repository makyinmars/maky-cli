pub mod exec;
pub mod ls;
pub mod read;
pub mod registry;

use std::path::PathBuf;

use async_trait::async_trait;

use crate::model::types::{ToolCall, ToolResult};

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
    pub approval_required: bool,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}

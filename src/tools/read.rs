use async_trait::async_trait;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler},
};

#[derive(Debug, Default)]
pub struct ReadFileTool;

#[async_trait]
impl ToolHandler for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let output = format!(
            "read_file is scaffolded only. workspace_root={} args={:?}",
            ctx.workspace_root.display(),
            call.args
        );

        Ok(ToolResult {
            call_id: call.id,
            output,
            success: false,
        })
    }
}

use async_trait::async_trait;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler},
};

#[derive(Debug, Default)]
pub struct ListFilesTool;

#[async_trait]
impl ToolHandler for ListFilesTool {
    fn name(&self) -> &'static str {
        "list_files"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let output = format!(
            "list_files is scaffolded only. workspace_root={} args={:?}",
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

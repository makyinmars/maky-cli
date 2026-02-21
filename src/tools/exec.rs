use async_trait::async_trait;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler},
};

#[derive(Debug, Default)]
pub struct ExecCommandTool;

#[async_trait]
impl ToolHandler for ExecCommandTool {
    fn name(&self) -> &'static str {
        "exec_command"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let output = format!(
            "exec_command is scaffolded only. workspace_root={}, approval_required={}",
            ctx.workspace_root.display(),
            ctx.approval_required
        );

        Ok(ToolResult {
            call_id: call.id,
            output,
            success: false,
        })
    }
}

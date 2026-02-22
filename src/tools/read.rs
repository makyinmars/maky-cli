use async_trait::async_trait;
use std::fs;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler, parse_json_args, resolve_workspace_path, truncate_output},
};

#[derive(Debug, Default)]
pub struct ReadFileTool;

#[async_trait]
impl ToolHandler for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let requested_path = parse_json_args(&call)
            .and_then(|args| {
                args.get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .or_else(|| call.args.first().cloned())
            .ok_or_else(|| anyhow::anyhow!("read_file requires `path` argument"))?;

        let file_path = resolve_workspace_path(&ctx.workspace_root, &requested_path)?;
        let content = fs::read_to_string(&file_path)
            .map_err(|error| anyhow::anyhow!("failed to read file: {error}"))?;
        let (output, truncated) = truncate_output(&content, ctx.max_output_chars);

        Ok(ToolResult {
            call_id: call.id,
            output,
            error: None,
            truncated,
            success: true,
        })
    }
}

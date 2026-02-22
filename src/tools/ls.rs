use async_trait::async_trait;
use std::fs;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler, parse_json_args, resolve_workspace_path, truncate_output},
};

#[derive(Debug, Default)]
pub struct ListFilesTool;

#[async_trait]
impl ToolHandler for ListFilesTool {
    fn name(&self) -> &'static str {
        "list_files"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let requested_path = parse_json_args(&call)
            .and_then(|args| {
                args.get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .or_else(|| call.args.first().cloned())
            .unwrap_or_else(|| ".".to_string());

        let directory_path = resolve_workspace_path(&ctx.workspace_root, &requested_path)?;
        let mut entries = fs::read_dir(&directory_path)
            .map_err(|error| anyhow::anyhow!("failed to read directory: {error}"))?
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    format!("{name}/")
                } else {
                    name
                }
            })
            .collect::<Vec<_>>();
        entries.sort();

        let output = if entries.is_empty() {
            "(empty directory)".to_string()
        } else {
            entries.join("\n")
        };
        let (output, truncated) = truncate_output(&output, ctx.max_output_chars);

        Ok(ToolResult {
            call_id: call.id,
            output,
            error: None,
            truncated,
            success: true,
        })
    }
}

use async_trait::async_trait;
use std::process::Command;

use crate::{
    model::types::{ToolCall, ToolResult},
    tools::{ToolContext, ToolHandler, parse_json_args, truncate_output},
};

#[derive(Debug, Default)]
pub struct ExecCommandTool;

#[async_trait]
impl ToolHandler for ExecCommandTool {
    fn name(&self) -> &'static str {
        "exec_command"
    }

    async fn execute(&self, call: ToolCall, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        if ctx.approval_required && !ctx.approval_granted {
            return Ok(ToolResult {
                call_id: call.id,
                output: "exec_command denied: explicit approval is required".to_string(),
                error: Some("approval required".to_string()),
                truncated: false,
                success: false,
            });
        }

        let command = parse_json_args(&call)
            .and_then(|args| {
                args.get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .or_else(|| call.args.first().cloned())
            .ok_or_else(|| anyhow::anyhow!("exec_command requires `command` argument"))?;

        let output_result = Command::new("sh")
            .arg("-lc")
            .arg(&command)
            .current_dir(&ctx.workspace_root)
            .output()
            .map_err(|error| anyhow::anyhow!("failed to spawn command: {error}"))?;

        let stdout = String::from_utf8_lossy(&output_result.stdout);
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        let merged = if stderr.trim().is_empty() {
            stdout.to_string()
        } else if stdout.trim().is_empty() {
            stderr.to_string()
        } else {
            format!("{stdout}\n[stderr]\n{stderr}")
        };
        let (output, truncated) = truncate_output(&merged, ctx.max_output_chars);
        let success = output_result.status.success();
        let error = if success {
            None
        } else {
            Some(format!(
                "command exited with status {}",
                output_result
                    .status
                    .code()
                    .map(|code: i32| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ))
        };

        Ok(ToolResult {
            call_id: call.id,
            output,
            error,
            truncated,
            success,
        })
    }
}

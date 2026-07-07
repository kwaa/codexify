use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ExecCommandArgs, ExecCommandOutput, exec_command, tool_schema};

pub const SHELL_COMMAND_TOOL_NAME: &str = "shell_command";

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ShellCommandArgs {
  /// Shell script to run.
  pub command: String,
  /// Optional working directory for the command.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub workdir: Option<String>,
  /// Optional shell binary. Defaults to `sh` on Unix and `cmd.exe` on Windows.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shell: Option<String>,
  /// Timeout in milliseconds. Defaults to 30000.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub timeout_ms: Option<u64>,
  /// Maximum bytes retained for each output stream. Defaults to 65536.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_output_bytes: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct ShellCommandTool;

impl ShellCommandTool {
  pub fn tool(&self) -> Tool {
    shell_command_tool()
  }

  pub async fn call(&self, args: ShellCommandArgs) -> CallToolResult {
    shell_command(args).await
  }
}

pub fn shell_command_tool() -> Tool {
  Tool::new(
    SHELL_COMMAND_TOOL_NAME,
    "Execute a local shell script using the legacy shell_command argument shape.",
    tool_schema::<ShellCommandArgs>(),
  )
  .with_raw_output_schema(tool_schema::<ExecCommandOutput>())
  .with_annotations(
    ToolAnnotations::new()
      .read_only(false)
      .destructive(true)
      .idempotent(false)
      .open_world(false),
  )
}

pub async fn shell_command(args: ShellCommandArgs) -> CallToolResult {
  exec_command(ExecCommandArgs {
    cmd: args.command,
    workdir: args.workdir,
    shell: args.shell,
    timeout_ms: args.timeout_ms,
    max_output_bytes: args.max_output_bytes,
  })
  .await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn shell_command_maps_legacy_command_argument() {
    let result = ShellCommandTool
      .call(ShellCommandArgs {
        command: "printf legacy".to_string(),
        workdir: None,
        shell: None,
        timeout_ms: Some(1_000),
        max_output_bytes: None,
      })
      .await;

    assert_eq!(result.is_error, Some(false));
    let structured = result.structured_content.expect("structured output");
    assert_eq!(structured["stdout"], "legacy");
  }
}

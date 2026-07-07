use std::time::Instant;

use rmcp::model::{CallToolResult, ContentBlock, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
  process::Command,
  time::{Duration, timeout},
};

use crate::tool_schema;

pub const EXEC_COMMAND_TOOL_NAME: &str = "exec_command";

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ExecCommandArgs {
  /// Shell command to execute.
  pub cmd: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ExecCommandOutput {
  pub command: String,
  pub exit_code: Option<i32>,
  pub success: bool,
  pub timed_out: bool,
  pub duration_ms: u128,
  pub stdout: String,
  pub stderr: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExecCommandTool;

impl ExecCommandTool {
  pub fn tool(&self) -> Tool {
    exec_command_tool()
  }

  pub async fn call(&self, args: ExecCommandArgs) -> CallToolResult {
    exec_command(args).await
  }
}

pub fn exec_command_tool() -> Tool {
  Tool::new(
    EXEC_COMMAND_TOOL_NAME,
    "Execute a local shell command and return exit status, stdout, stderr, and timing metadata.",
    tool_schema::<ExecCommandArgs>(),
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

pub async fn exec_command(args: ExecCommandArgs) -> CallToolResult {
  if args.cmd.trim().is_empty() {
    return error_result("cmd must not be empty");
  }

  let started = Instant::now();
  let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
  let max_output_bytes = args.max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);

  let mut command = shell_command(args.shell.as_deref(), &args.cmd);
  command.kill_on_drop(true);
  if let Some(workdir) = args.workdir.as_deref() {
    command.current_dir(workdir);
  }

  let output = match timeout(Duration::from_millis(timeout_ms), command.output()).await {
    Ok(Ok(output)) => {
      let result = ExecCommandOutput {
        command: args.cmd,
        exit_code: output.status.code(),
        success: output.status.success(),
        timed_out: false,
        duration_ms: started.elapsed().as_millis(),
        stdout: decode_and_truncate(&output.stdout, max_output_bytes),
        stderr: decode_and_truncate(&output.stderr, max_output_bytes),
      };
      return result_to_call_tool_result(result);
    }
    Ok(Err(err)) => {
      return error_result(format!("failed to execute command: {err}"));
    }
    Err(_) => ExecCommandOutput {
      command: args.cmd,
      exit_code: None,
      success: false,
      timed_out: true,
      duration_ms: started.elapsed().as_millis(),
      stdout: String::new(),
      stderr: format!("command timed out after {timeout_ms} ms"),
    },
  };

  result_to_call_tool_result(output)
}

fn shell_command(shell: Option<&str>, cmd: &str) -> Command {
  #[cfg(windows)]
  {
    let mut command = Command::new(shell.unwrap_or("cmd.exe"));
    command.arg("/C").arg(cmd);
    command
  }

  #[cfg(not(windows))]
  {
    let mut command = Command::new(shell.unwrap_or("sh"));
    command.arg("-c").arg(cmd);
    command
  }
}

fn result_to_call_tool_result(output: ExecCommandOutput) -> CallToolResult {
  let text = format_exec_output(&output);
  let is_error = !output.success;
  let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
  result.structured_content = Some(json!(output));
  result.is_error = Some(is_error);
  result
}

fn error_result(message: impl Into<String>) -> CallToolResult {
  CallToolResult::error(vec![ContentBlock::text(message.into())])
}

fn format_exec_output(output: &ExecCommandOutput) -> String {
  let exit = output
    .exit_code
    .map_or_else(|| "none".to_string(), |code| code.to_string());
  format!(
    "Exit code: {exit}\nTimed out: {}\nDuration: {} ms\nStdout:\n{}\nStderr:\n{}",
    output.timed_out, output.duration_ms, output.stdout, output.stderr
  )
}

fn decode_and_truncate(bytes: &[u8], max_bytes: usize) -> String {
  if bytes.len() <= max_bytes {
    return String::from_utf8_lossy(bytes).into_owned();
  }

  let mut text = String::from_utf8_lossy(&bytes[..max_bytes]).into_owned();
  text.push_str(&format!(
    "\n[output truncated: kept {max_bytes} of {} bytes]",
    bytes.len()
  ));
  text
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tool_definition_uses_rmcp_tool_model() {
    let tool = ExecCommandTool.tool();

    assert_eq!(tool.name, EXEC_COMMAND_TOOL_NAME);
    assert!(tool.schema_as_json_value()["properties"]["cmd"].is_object());
  }

  #[tokio::test]
  async fn exec_command_returns_structured_output() {
    let result = ExecCommandTool
      .call(ExecCommandArgs {
        cmd: "printf hello".to_string(),
        workdir: None,
        shell: None,
        timeout_ms: Some(1_000),
        max_output_bytes: None,
      })
      .await;

    assert_eq!(result.is_error, Some(false));
    let structured = result.structured_content.expect("structured output");
    assert_eq!(structured["stdout"], "hello");
    assert_eq!(structured["success"], true);
  }
}

//! Tool definitions and execution logic for Codexify.
//!
//! This crate intentionally does not host an MCP server. It exposes tools as
//! reusable values and functions so callers can register them with rmcp, tests,
//! or another runtime boundary.

use std::sync::Arc;

use rmcp::model::JsonObject;
use schemars::JsonSchema;
use serde_json::Value;

mod apply_patch;
mod exec_command;
mod shell_command;
mod view_image;

pub use apply_patch::{
  APPLY_PATCH_TOOL_NAME, ApplyPatchArgs, ApplyPatchOutput, ApplyPatchTool, apply_patch,
  apply_patch_tool,
};
pub use exec_command::{
  EXEC_COMMAND_TOOL_NAME, ExecCommandArgs, ExecCommandOutput, ExecCommandTool, exec_command,
  exec_command_tool,
};
pub use shell_command::{
  SHELL_COMMAND_TOOL_NAME, ShellCommandArgs, ShellCommandTool, shell_command, shell_command_tool,
};
pub use view_image::{
  VIEW_IMAGE_TOOL_NAME, ViewImageArgs, ViewImageTool, view_image, view_image_tool,
};

pub(crate) fn tool_schema<T>() -> Arc<JsonObject>
where
  T: JsonSchema,
{
  let schema = serde_json::to_value(schemars::schema_for!(T))
    .expect("schemars-generated schema should serialize to JSON");
  match schema {
    Value::Object(object) => Arc::new(object),
    _ => unreachable!("schemars root schema is a JSON object"),
  }
}

use std::future::Future;

use codexify_tools::{
  APPLY_PATCH_TOOL_NAME, EXEC_COMMAND_TOOL_NAME, SHELL_COMMAND_TOOL_NAME, VIEW_IMAGE_TOOL_NAME,
  apply_patch, apply_patch_tool, exec_command, exec_command_tool, shell_command,
  shell_command_tool, view_image, view_image_tool,
};
use rmcp::{
  ErrorData as McpError, ServerHandler,
  model::{
    CallToolRequestParams, CallToolResult, ErrorCode, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
  },
  service::{MaybeSendFuture, RequestContext, RoleServer},
};

#[derive(Debug, Clone, Default)]
pub struct CodexifyServer;

impl CodexifyServer {
  pub fn new() -> Self {
    Self
  }
}

pub fn codexify_tools() -> Vec<Tool> {
  vec![
    exec_command_tool(),
    shell_command_tool(),
    apply_patch_tool(),
    view_image_tool(),
  ]
}

impl ServerHandler for CodexifyServer {
  fn get_info(&self) -> ServerInfo {
    ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
      .with_server_info(Implementation::new("codexify", env!("CARGO_PKG_VERSION")))
      .with_instructions("Codexify local tools server")
  }

  fn list_tools(
    &self,
    _request: Option<PaginatedRequestParams>,
    _context: RequestContext<RoleServer>,
  ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
    std::future::ready(Ok(ListToolsResult::with_all_items(codexify_tools())))
  }

  fn get_tool(&self, name: &str) -> Option<Tool> {
    codexify_tools()
      .into_iter()
      .find(|tool| tool.name.as_ref() == name)
  }

  async fn call_tool(
    &self,
    request: CallToolRequestParams,
    _context: RequestContext<RoleServer>,
  ) -> Result<CallToolResult, McpError> {
    let arguments = request.arguments.unwrap_or_default();
    match request.name.as_ref() {
      EXEC_COMMAND_TOOL_NAME => {
        let args = parse_arguments(arguments)?;
        Ok(exec_command(args).await)
      }
      SHELL_COMMAND_TOOL_NAME => {
        let args = parse_arguments(arguments)?;
        Ok(shell_command(args).await)
      }
      APPLY_PATCH_TOOL_NAME => {
        let args = parse_arguments(arguments)?;
        Ok(apply_patch(args))
      }
      VIEW_IMAGE_TOOL_NAME => {
        let args = parse_arguments(arguments)?;
        Ok(view_image(args))
      }
      name => Err(McpError::new(
        ErrorCode::METHOD_NOT_FOUND,
        format!("unknown tool `{name}`"),
        None,
      )),
    }
  }
}

fn parse_arguments<T>(arguments: rmcp::model::JsonObject) -> Result<T, McpError>
where
  T: serde::de::DeserializeOwned,
{
  serde_json::from_value(serde_json::Value::Object(arguments)).map_err(|err| {
    McpError::invalid_params(format!("failed to deserialize tool arguments: {err}"), None)
  })
}

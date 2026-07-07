use rmcp::{
  RoleServer, ServiceExt,
  service::{RunningService, ServerInitializeError},
  transport::stdio,
};

use crate::CodexifyServer;

pub async fn serve_stdio()
-> Result<RunningService<RoleServer, CodexifyServer>, ServerInitializeError> {
  CodexifyServer::new().serve(stdio()).await
}

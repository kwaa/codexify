use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
  StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::CodexifyServer;

pub fn serve_http(config: StreamableHttpServerConfig) -> StreamableHttpService<CodexifyServer, LocalSessionManager> {
  StreamableHttpService::new(
    || Ok(CodexifyServer::new()),
    Arc::new(LocalSessionManager::default()),
    config,
  )
}

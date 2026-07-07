//! MCP server wiring for Codexify tools.
//!
//! This crate is a library only. It exposes a tools-only MCP service and
//! transport helpers for stdio and streamable HTTP/SSE.

mod http;
mod service;
mod stdio;

pub use http::serve_http;
pub use service::{CodexifyServer, codexify_tools};
pub use stdio::serve_stdio;

//! sapphire-tally MCP サーバー。`server` モジュールが MCP ツールを、
//! `http` モジュールが streamable HTTP トランスポートを提供する。

pub mod http;
pub mod server;

pub use http::serve_http;
pub use server::TallyServer;

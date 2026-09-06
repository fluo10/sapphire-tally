//! streamable HTTP トランスポート。sapphire-journal-mcp の http.rs と同じ方針:
//! ループバックのホスト名は常に許可し、`--allowed-host` で追加許可を設定する。
//! bind をループバック以外に広げると Host 検証で弾かれるようになるため、
//! 必ず `--allowed-host` も併せて設定すること（空リスト＝全許可にならない）。

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use crate::server::TallyServer;
use sapphire_tally_core::Tally;

/// ループバック由来のリクエストは常に許可するホスト名。
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Tally データディレクトリを開き、MCP サーバーを `/mcp` にマウントして HTTP で提供する。
pub async fn serve_http(
    data_dir: &Path,
    addr: &str,
    port: u16,
    allowed_hosts: &[String],
) -> anyhow::Result<()> {
    let state = Arc::new(Mutex::new(
        Tally::open(data_dir.to_path_buf()).context("failed to open tally data dir")?,
    ));

    let mut hosts: Vec<String> = LOOPBACK_HOSTS.iter().map(|s| s.to_string()).collect();
    hosts.extend(allowed_hosts.iter().cloned());

    let service = StreamableHttpService::new(
        move || Ok(TallyServer::from_shared(Arc::clone(&state))),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(CancellationToken::new())
            .with_allowed_hosts(hosts),
    );

    let app = axum::Router::new().route_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind((addr, port)).await?;
    tracing::info!(%addr, port, "sapphire-tally MCP server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

use clap::Parser as _;

#[derive(clap::Parser)]
#[command(
    name = "sapphire-tally-server",
    about = "MCP server for sapphire-tally"
)]
struct Cli {
    /// データディレクトリ（stamps/ と activities/ を含む）。環境変数 SAPPHIRE_TALLY_DIR でも指定可。
    #[arg(long, env = "SAPPHIRE_TALLY_DIR")]
    data_dir: Option<String>,
    #[arg(long, default_value = "127.0.0.1")]
    addr: String,
    #[arg(long, default_value_t = 3174)]
    port: u16,
    /// MCP Host allowlist に追加するホスト名（繰り返し指定可）
    #[arg(long)]
    allowed_host: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let data_dir = cli
        .data_dir
        .ok_or_else(|| anyhow::anyhow!("--data-dir is required (or set SAPPHIRE_TALLY_DIR)"))?;
    sapphire_tally_server::serve_http(
        std::path::Path::new(&data_dir),
        &cli.addr,
        cli.port,
        &cli.allowed_host,
    )
    .await
}

use std::{env, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::Router;
use enhanced_gmail_mcp::{
    configuration::{ConfigurationState, router as configuration_router},
    gmail::GmailClient,
    server::GmailMcpServer,
    summary::Summarizer,
};
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let openai_secret_path = PathBuf::from(env_value(
        "OPENAI_API_KEY_FILE",
        "/run/secrets/openai_api_key",
    ));
    let groq_secret_path =
        PathBuf::from(env_value("GROQ_API_KEY_FILE", "/run/secrets/groq_api_key"));
    let groq = Some((
        groq_secret_path.as_path(),
        env_value("GROQ_MODEL", "openai/gpt-oss-20b"),
    ));
    let configuration = ConfigurationState::new(
        &env_value("DATABASE_URL", "sqlite:///data/gmail-mcp.db?mode=rwc"),
        required_env("CONFIG_BASIC_USERNAME")?,
        required_env("CONFIG_BASIC_PASSWORD")?,
    )
    .await
    .context("failed to initialize configuration storage")?;
    let summarizer = Summarizer::from_secret_files(&openai_secret_path, groq, configuration.pool())
        .await
        .context("failed to initialize summarizer")?;
    let gmail = GmailClient::from_env(summarizer)
        .await
        .context("failed to initialize Gmail")?;

    let gmail_for_sessions = gmail.clone();
    let allowed_hosts = env_value("MCP_ALLOWED_HOSTS", "localhost,127.0.0.1,::1,gmail-mcp")
        .split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mcp = StreamableHttpService::new(
        move || Ok(GmailMcpServer::new(gmail_for_sessions.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts),
    );

    let app = Router::new()
        .nest_service("/mcp", mcp)
        .merge(configuration_router(configuration));
    let address = env_value("BIND_ADDRESS", "0.0.0.0:3000");
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("failed to bind {address}"))?;
    tracing::info!(%address, "Gmail MCP listening");
    axum::serve(listener, app)
        .await
        .context("HTTP server failed")?;
    Ok(())
}

fn env_value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} must be set"))
}

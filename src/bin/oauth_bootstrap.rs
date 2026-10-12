use std::path::PathBuf;

use anyhow::{Context, Result};
use google_gmail1::{hyper_rustls, hyper_util, yup_oauth2};

const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.labels",
    "https://www.googleapis.com/auth/gmail.modify",
];

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let credentials = PathBuf::from(
        args.next()
            .unwrap_or_else(|| ".secrets/client_secret.json".into()),
    );
    let token_cache = PathBuf::from(args.next().unwrap_or_else(|| "token_cache.json".into()));
    anyhow::ensure!(
        args.next().is_none(),
        "usage: oauth_bootstrap [client-secret] [token-cache]"
    );

    let secret = yup_oauth2::read_application_secret(&credentials)
        .await
        .with_context(|| format!("failed to read {}", credentials.display()))?;
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()?
        .https_only()
        .enable_http2()
        .build();
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(connector);
    let authenticator = yup_oauth2::InstalledFlowAuthenticator::with_client(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        yup_oauth2::client::CustomHyperClientBuilder::from(client),
    )
    .persist_tokens_to_disk(&token_cache)
    .build()
    .await?;
    authenticator.token(SCOPES).await?;
    println!("OAuth token cache written to {}", token_cache.display());
    Ok(())
}

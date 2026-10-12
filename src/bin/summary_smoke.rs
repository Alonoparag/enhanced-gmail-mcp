use std::path::Path;

use anyhow::{Context, ensure};
use enhanced_gmail_mcp::{
    configuration::ConfigurationState, contracts::SummaryStatus, summary::Summarizer,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sqlite://.runtime-data/summary-smoke.db?mode=rwc".into());
    let configuration = ConfigurationState::new(&database_url, "smoke".into(), "smoke".into())
        .await
        .context("failed to initialize smoke-test SQLite")?;
    let summarizer = Summarizer::from_secret_files(
        Path::new(".secrets/openai_api_key"),
        Some((
            Path::new(".secrets/groq_api_key"),
            "openai/gpt-oss-20b".into(),
        )),
        configuration.pool(),
    )
    .await?;

    let input =
        "Synthetic email: The project review is Tuesday at 10:00. Bring the release checklist.";
    let generated = summarizer.summarize(input).await;
    ensure!(
        generated.status == SummaryStatus::Generated,
        "first summary was not generated"
    );
    ensure!(
        generated
            .text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
        "Groq returned an empty summary"
    );

    let cached = summarizer.summarize(input).await;
    ensure!(
        cached.status == SummaryStatus::Cached,
        "second summary did not use SQLite"
    );
    ensure!(cached.text == generated.text, "cached summary changed");
    println!(
        "Groq summary generated and reused from SQLite; model={}",
        generated.model
    );
    Ok(())
}

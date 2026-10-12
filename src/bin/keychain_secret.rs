#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
        os::unix::fs::OpenOptionsExt,
        path::PathBuf,
    };

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("self-test") => {
            let account = format!("self-test-{}", uuid::Uuid::new_v4());
            let entry = keyring::Entry::new("enhanced-gmail-mcp", &account)?;
            entry.set_password("keyring-self-test")?;
            let result = (entry.get_password()? == "keyring-self-test")
                .then_some(())
                .ok_or_else(|| anyhow::anyhow!("Keychain returned a different value"));
            entry.delete_credential()?;
            result?;
            println!("macOS Keychain self-test passed");
        }
        Some("store") => {
            let account = account(args.next().as_deref())?;
            let mut key = String::new();
            std::io::stdin().read_to_string(&mut key)?;
            anyhow::ensure!(!key.trim().is_empty(), "API key must be provided on stdin");
            let user = std::env::var("USER")?;
            keyring::Entry::new(account, &user)?.set_password(key.trim())?;
        }
        Some("export") => {
            let provider = args.next();
            let account = account(provider.as_deref())?;
            let path = PathBuf::from(args.next().unwrap_or_else(|| {
                format!(
                    ".secrets/{}_api_key",
                    provider.as_deref().unwrap_or("openai")
                )
            }));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let user = std::env::var("USER")?;
            let key = keyring::Entry::new(account, &user)?.get_password()?;
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(key.as_bytes())?;
        }
        _ => anyhow::bail!(
            "usage: keychain-secret <self-test|store <openai|groq>|export <openai|groq> [path]>"
        ),
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn account(provider: Option<&str>) -> anyhow::Result<&'static str> {
    match provider {
        Some("openai") => Ok("OPENAI_API_KEY"),
        Some("groq") => Ok("GROQ_API_KEY"),
        _ => anyhow::bail!("provider must be openai or groq"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("keychain-secret is supported on macOS only");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt, path::PathBuf};

    let mut args = std::env::args().skip(1);
    let runtime_data = PathBuf::from(args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "usage: prepare_host <runtime-data-directory> <openai|groq|both> [token-cache]"
        )
    })?);
    let provider = args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "usage: prepare_host <runtime-data-directory> <openai|groq|both> [token-cache]"
        )
    })?;
    let token_source = PathBuf::from(args.next().unwrap_or_else(|| "token_cache.json".into()));
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");

    let metadata = std::fs::metadata(&token_source)?;
    anyhow::ensure!(
        metadata.is_file(),
        "OAuth token cache must be a regular file"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "OAuth token cache permissions must not allow group or other access"
    );

    let providers: &[(&str, &str)] = match provider.as_str() {
        "openai" => &[("openai", "OPENAI_API_KEY")],
        "groq" => &[("groq", "GROQ_API_KEY")],
        "both" => &[("openai", "OPENAI_API_KEY"), ("groq", "GROQ_API_KEY")],
        _ => anyhow::bail!("provider must be openai, groq, or both"),
    };

    let credentials = providers
        .iter()
        .map(|(name, account)| {
            let path = PathBuf::from(format!(".secrets/{name}_api_key"));
            if path.exists() {
                validate_private_file(&path)?;
                return Ok((*name, None));
            }
            let bundle = PathBuf::from(".secrets/.secrets");
            if bundle.exists() {
                validate_private_file(&bundle)?;
                let key = read_env_secret(&bundle, account)?;
                return Ok((*name, Some(key)));
            }
            let key = read_keychain_secret(account)?;
            anyhow::ensure!(!key.trim().is_empty(), "configured provider key is empty");
            Ok((*name, Some(key)))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    std::fs::create_dir_all(".secrets")?;
    for (name, key) in credentials {
        if let Some(key) = key {
            write_private(
                &PathBuf::from(format!(".secrets/{name}_api_key")),
                key.trim().as_bytes(),
            )?;
        }
    }

    std::fs::create_dir_all(&runtime_data)?;
    std::fs::set_permissions(&runtime_data, Permissions::from_mode(0o700))?;
    let token = std::fs::read(&token_source)?;
    write_private(&runtime_data.join("token_cache.json"), &token)?;
    println!(
        "Host artifacts prepared for {provider} in {}",
        runtime_data.display()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_env_secret(path: &std::path::Path, name: &str) -> anyhow::Result<String> {
    let contents = std::fs::read_to_string(path)?;
    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().trim_matches(['\'', '"']).to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{name} is missing from the provider secret bundle"))
}

#[cfg(target_os = "macos")]
fn validate_private_file(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)?;
    anyhow::ensure!(metadata.is_file(), "provider secret must be a regular file");
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "provider secret permissions must not allow group or other access"
    );
    anyhow::ensure!(metadata.len() > 0, "provider secret must not be empty");
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_keychain_secret(account: &str) -> anyhow::Result<String> {
    let user = std::env::var("USER").unwrap_or_default();
    if !user.is_empty()
        && let Ok(value) = keyring::Entry::new(account, &user)?.get_password()
    {
        return Ok(value);
    }
    if !user.is_empty() {
        let output = std::process::Command::new("security")
            .args(["find-generic-password", "-s", account, "-a", &user, "-w"])
            .output()?;
        if output.status.success() {
            return String::from_utf8(output.stdout).map_err(Into::into);
        }
    }
    match keyring::Entry::new("enhanced-gmail-mcp", account)?.get_password() {
        Ok(value) => Ok(value),
        Err(primary_error) => {
            let legacy = match account {
                "OPENAI_API_KEY" => "openai-api-key",
                "GROQ_API_KEY" => "groq-api-key",
                _ => return Err(primary_error.into()),
            };
            keyring::Entry::new("enhanced-gmail-mcp", legacy)?
                .get_password()
                .map_err(Into::into)
        }
    }
}

#[cfg(target_os = "macos")]
fn write_private(path: &std::path::Path, value: &[u8]) -> anyhow::Result<()> {
    use std::{
        fs::{OpenOptions, Permissions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    file.write_all(value)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("prepare_host is supported on macOS only");
    std::process::exit(1);
}

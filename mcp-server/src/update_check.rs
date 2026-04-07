// Background update checker
//
// Opt-in via JDWP_MCP_UPDATE_CHECK=true env var.
// Tries pip first (works if installed via pip), falls back to git ls-remote.
// Caches result for 24h. Writes to stderr only if update available.

use std::time::Duration;
use tracing::info;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn spawn_update_check() {
    tokio::task::spawn_blocking(|| {
        if let Err(e) = check_update() {
            tracing::debug!("Update check failed: {}", e);
        }
    });
}

fn check_update() -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = cache_path();
    let cache_file = cache_dir.join("last-check");
    let version_file = cache_dir.join("latest-version");

    // Use cached result if checked within last 24h
    if let Ok(meta) = std::fs::metadata(&cache_file) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(86400) {
                if let Ok(cached) = std::fs::read_to_string(&version_file) {
                    let latest = cached.trim().to_string();
                    if !latest.is_empty() && latest != CURRENT_VERSION {
                        notify(&latest);
                    }
                }
                return Ok(());
            }
        }
    }

    // Try pip first, then git
    let latest = fetch_via_pip().or_else(|_| fetch_via_git())?;

    // Cache
    let _ = std::fs::create_dir_all(&cache_dir);
    let _ = std::fs::write(&version_file, &latest);
    let _ = std::fs::File::create(&cache_file);

    if latest != CURRENT_VERSION {
        notify(&latest);
    }

    Ok(())
}

/// Check PyPI via `pip index versions jdwp-mcp`
fn fetch_via_pip() -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("pip")
        .args(["index", "versions", "jdwp-mcp"])
        .stderr(std::process::Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err("pip index failed".into());
    }

    // Output: "jdwp-mcp (0.3.0)\n  Available versions: 0.3.0, 0.2.2, ..."
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()
        .and_then(|line| {
            let start = line.find('(')?;
            let end = line.find(')')?;
            Some(line[start + 1..end].to_string())
        })
        .ok_or("could not parse pip output")?;

    Ok(version)
}

/// Fallback: check GitHub tags via `git ls-remote`
fn fetch_via_git() -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args([
            "ls-remote",
            "--tags",
            "--sort=-v:refname",
            "https://github.com/dronsv/jdwp-mcp",
        ])
        .stderr(std::process::Stdio::null())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let tag = line.rsplit('/').next()?;
            if tag.starts_with('v') && !tag.contains("^{}") {
                Some(tag.trim_start_matches('v').to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| "no tags found".into())
}

fn notify(latest: &str) {
    info!(
        "Update available: {} -> {} — run: pip install --upgrade jdwp-mcp",
        CURRENT_VERSION, latest
    );
}

fn cache_path() -> std::path::PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(|p| std::path::PathBuf::from(p).join("jdwp-mcp"))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".cache").join("jdwp-mcp"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/jdwp-mcp-cache"))
}

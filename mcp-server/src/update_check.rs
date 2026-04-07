// Background update checker
//
// Checks GitHub for a newer version tag. Non-blocking, runs once on startup.
// Writes to stderr (via tracing) only if an update is available. Never fails visibly.
// Caches result for 24h to avoid repeated checks.

use std::time::Duration;
use tracing::info;

const REPO: &str = "dronsv/jdwp-mcp";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spawn a background check (non-blocking, best-effort)
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

    // Fetch latest tag via git ls-remote (works without auth, no TLS library needed)
    let output = std::process::Command::new("git")
        .args([
            "ls-remote",
            "--tags",
            "--sort=-v:refname",
            &format!("https://github.com/{}", REPO),
        ])
        .stderr(std::process::Stdio::null())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let latest = stdout
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
        .ok_or("no tags found")?;

    // Cache
    let _ = std::fs::create_dir_all(&cache_dir);
    let _ = std::fs::write(&version_file, &latest);
    let _ = std::fs::File::create(&cache_file);

    if latest != CURRENT_VERSION {
        notify(&latest);
    }

    Ok(())
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

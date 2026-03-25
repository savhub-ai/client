/// Auto-update: check GitHub Releases, download, replace binary, restart.
use semver::Version;
use serde::Deserialize;

const GITHUB_REPO: &str = "savhub-ai/client";
const GITHUB_API: &str = "https://api.github.com";

/// Current binary version (from Cargo.toml at compile time).
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("invalid CARGO_PKG_VERSION")
}

/// Status of the auto-update lifecycle.
#[derive(Clone, PartialEq)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    Available {
        version: String,
        download_url: String,
        asset_name: String,
    },
    Downloading,
    ReadyToRestart,
    Failed(String),
}

// --- GitHub API types ---

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

// --- Platform detection ---

/// Returns the expected asset filename for the current platform.
fn platform_asset_name() -> Option<String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    Some(format!("savhub-desktop-{os}-{arch}.{ext}"))
}

// --- Public API ---

/// Check GitHub Releases for a newer version. Returns `None` if up-to-date.
pub async fn check_for_update() -> Result<Option<(String, String, String)>, String> {
    let asset_name = platform_asset_name().ok_or("Unsupported platform")?;

    let client = reqwest::Client::new();
    let url = format!("{GITHUB_API}/repos/{GITHUB_REPO}/releases/latest");

    let resp = client
        .get(&url)
        .header("User-Agent", "savhub-desktop")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let release: GitHubRelease = resp.json().await.map_err(|e| e.to_string())?;

    // Skip pre-releases
    if release.prerelease {
        return Ok(None);
    }

    let tag = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let remote_version = Version::parse(tag).map_err(|e| e.to_string())?;
    let local_version = current_version();

    if remote_version <= local_version {
        return Ok(None);
    }

    // Find matching asset
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| format!("No asset found for {asset_name}"))?;

    Ok(Some((
        remote_version.to_string(),
        asset.browser_download_url.clone(),
        asset.name.clone(),
    )))
}

/// Download the archive and replace the running binary.
pub async fn download_and_install(download_url: &str, asset_name: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let bytes = client
        .get(download_url)
        .header("User-Agent", "savhub-desktop")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;

    // Extract binary from archive
    let new_binary = if asset_name.ends_with(".zip") {
        extract_from_zip(&bytes)?
    } else {
        extract_from_tar_gz(&bytes)?
    };

    // Replace current binary
    replace_binary(&exe_path, &new_binary)?;

    Ok(())
}

/// Launch a new instance and exit.
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(&exe).spawn();
        std::process::exit(0);
    }
}

/// Remove the `.old` backup left by a previous update.
pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent() {
            let old_name = if cfg!(windows) {
                "savhub-desktop.old.exe"
            } else {
                "savhub-desktop.old"
            };
            let _ = std::fs::remove_file(parent.join(old_name));
        }
}

// --- Internal helpers ---

fn extract_from_zip(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let binary_name = if cfg!(windows) {
        "savhub-desktop.exe"
    } else {
        "savhub-desktop"
    };

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        if name == binary_name || name.ends_with(&format!("/{binary_name}")) {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            return Ok(buf);
        }
    }
    Err(format!("{binary_name} not found in archive"))
}

fn extract_from_tar_gz(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;

    use flate2::read::GzDecoder;

    let decoder = GzDecoder::new(data);
    let mut archive = self_update_tar::Archive::new(decoder);

    let binary_name = "savhub-desktop";

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        let matches = path.file_name().map(|n| n.to_string_lossy()).as_deref() == Some(binary_name);
        if matches {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            return Ok(buf);
        }
    }
    Err(format!("{binary_name} not found in archive"))
}

fn replace_binary(exe_path: &std::path::Path, new_binary: &[u8]) -> Result<(), String> {
    let parent = exe_path
        .parent()
        .ok_or("Cannot determine binary directory")?;

    let old_name = if cfg!(windows) {
        "savhub-desktop.old.exe"
    } else {
        "savhub-desktop.old"
    };
    let backup_path = parent.join(old_name);

    // Remove stale backup
    let _ = std::fs::remove_file(&backup_path);

    // Rename running binary → backup (Windows allows rename of a running exe)
    std::fs::rename(exe_path, &backup_path)
        .map_err(|e| format!("Failed to backup current binary: {e}"))?;

    // Write new binary
    if let Err(e) = std::fs::write(exe_path, new_binary) {
        // Attempt rollback
        let _ = std::fs::rename(&backup_path, exe_path);
        return Err(format!("Failed to write new binary: {e}"));
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(exe_path, std::fs::Permissions::from_mode(0o755));
    }

    Ok(())
}

/// Self-update: check GitHub Releases, download, replace the CLI binary.
use semver::Version;
use serde::Deserialize;

const GITHUB_REPO: &str = "savhub-ai/client";
const GITHUB_API: &str = "https://api.github.com";

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("invalid CARGO_PKG_VERSION")
}

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
    Some(format!("savhub-cli-{os}-{arch}.{ext}"))
}

pub async fn run_self_update() -> anyhow::Result<()> {
    let local_version = current_version();
    println!("Current version: {local_version}");

    // --- Check for update ---
    print!("Checking for updates... ");
    let asset_name =
        platform_asset_name().ok_or_else(|| anyhow::anyhow!("unsupported platform"))?;

    let client = reqwest::Client::new();
    let url = format!("{GITHUB_API}/repos/{GITHUB_REPO}/releases/latest");

    let resp = client
        .get(&url)
        .header("User-Agent", "savhub-cli")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("GitHub API returned {}", resp.status());
    }

    let release: GitHubRelease = resp.json().await?;

    if release.prerelease {
        println!("latest release is a pre-release, skipping.");
        return Ok(());
    }

    let tag = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let remote_version = Version::parse(tag)?;

    if remote_version <= local_version {
        println!("already up-to-date.");
        return Ok(());
    }

    println!("found {remote_version}");

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| anyhow::anyhow!("no release asset for {asset_name}"))?;

    // --- Download ---
    println!("Downloading {}...", asset.name);
    let bytes = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "savhub-cli")
        .send()
        .await?
        .bytes()
        .await?;

    // --- Extract ---
    let new_binary = if asset.name.ends_with(".zip") {
        extract_from_zip(&bytes)?
    } else {
        extract_from_tar_gz(&bytes)?
    };

    // --- Replace ---
    let exe_path = std::env::current_exe()?;
    replace_binary(&exe_path, &new_binary)?;

    println!("Updated to {remote_version}. Restart savhub to use the new version.");
    Ok(())
}

pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent() {
            let old_name = if cfg!(windows) {
                "savhub.old.exe"
            } else {
                "savhub.old"
            };
            let _ = std::fs::remove_file(parent.join(old_name));
        }
}

// --- Internal helpers ---

fn extract_from_zip(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let binary_name = if cfg!(windows) {
        "savhub.exe"
    } else {
        "savhub"
    };

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name == binary_name || name.ends_with(&format!("/{binary_name}")) {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("{binary_name} not found in archive")
}

fn extract_from_tar_gz(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;

    use flate2::read::GzDecoder;

    let decoder = GzDecoder::new(data);
    let mut archive = self_update_tar::Archive::new(decoder);

    let binary_name = "savhub";

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let matches = path.file_name().map(|n| n.to_string_lossy()).as_deref() == Some(binary_name);
        if matches {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("{binary_name} not found in archive")
}

fn replace_binary(exe_path: &std::path::Path, new_binary: &[u8]) -> anyhow::Result<()> {
    let parent = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine binary directory"))?;

    let old_name = if cfg!(windows) {
        "savhub.old.exe"
    } else {
        "savhub.old"
    };
    let backup_path = parent.join(old_name);

    // Remove stale backup
    let _ = std::fs::remove_file(&backup_path);

    // Rename running binary -> backup (Windows allows rename of a running exe)
    std::fs::rename(exe_path, &backup_path)
        .map_err(|e| anyhow::anyhow!("failed to backup current binary: {e}"))?;

    // Write new binary
    if let Err(e) = std::fs::write(exe_path, new_binary) {
        // Attempt rollback
        let _ = std::fs::rename(&backup_path, exe_path);
        anyhow::bail!("failed to write new binary: {e}");
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(exe_path, std::fs::Permissions::from_mode(0o755));
    }

    Ok(())
}

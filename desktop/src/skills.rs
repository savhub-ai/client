use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use zip::ZipArchive;

/// Extract a ZIP bundle into the given directory.
pub fn extract_zip(bytes: &[u8], target: &Path) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let out = target.join(name);
        if file.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(out, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Update the lockfile with the fetched skill version.
pub fn update_lockfile(workdir: &Path, slug: &str, version: &str) {
    let lock_dir = workdir.join(".savhub");
    let _ = std::fs::create_dir_all(&lock_dir);
    let lock_path = lock_dir.join("lock.json");

    let mut lock: serde_json::Value = std::fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({"version": 1, "skills": {}}));

    if let Some(skills) = lock.get_mut("skills").and_then(|v| v.as_object_mut()) {
        skills.insert(
            slug.to_string(),
            serde_json::json!({
                "version": version,
                "fetched_at": chrono::Utc::now().timestamp()
            }),
        );
    }

    let _ = std::fs::write(
        &lock_path,
        serde_json::to_string_pretty(&lock).unwrap_or_default(),
    );
}

/// Read fetched skill versions from the desktop lockfile.
pub fn read_fetched_skill_versions(workdir: &Path) -> BTreeMap<String, String> {
    let lock_path = workdir.join(".savhub").join("lock.json");
    std::fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|lock| lock.get("skills").and_then(|v| v.as_object()).cloned())
        .map(|skills| {
            skills
                .into_iter()
                .map(|(slug, value)| {
                    let version = value
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("fetched")
                        .to_string();
                    (slug, version)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Remove a fetched skill folder and update the desktop lockfile.
pub fn prune_skill(workdir: &Path, slug: &str) -> Result<(), String> {
    let skill_dir = workdir.join(slug);
    match std::fs::remove_dir_all(&skill_dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.to_string()),
    }

    let lock_dir = workdir.join(".savhub");
    let lock_path = lock_dir.join("lock.json");
    let mut lock: serde_json::Value = std::fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({"version": 1, "skills": {}}));

    if let Some(skills) = lock.get_mut("skills").and_then(|v| v.as_object_mut()) {
        skills.remove(slug);
    }

    std::fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &lock_path,
        serde_json::to_string_pretty(&lock).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

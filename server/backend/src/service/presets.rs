use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use shared::PresetsResponse;

/// Embedded preset selectors (compiled into the binary from the repo root
/// `presets.json`).  The server serves this payload verbatim, with an ETag
/// derived from the SHA-256 hash so clients can do conditional fetches.
static PRESETS: Lazy<PresetPayload> = Lazy::new(|| {
    let raw = include_str!("../../../../presets.json");
    let parsed: serde_json::Value =
        serde_json::from_str(raw).expect("presets.json is not valid JSON");
    let presets_array = parsed
        .get("presets")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let version = parsed
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let hash = Sha256::digest(raw.as_bytes());
    let hex: String = hash[..16].iter().map(|b| format!("{b:02x}")).collect();
    let etag = format!("\"sha256-{hex}\"");

    let response = PresetsResponse {
        version,
        etag: Some(etag.clone()),
        presets: presets_array,
    };
    let json = serde_json::to_string(&response).expect("failed to serialise presets response");

    PresetPayload { json, etag }
});

struct PresetPayload {
    json: String,
    etag: String,
}

/// Return the cached preset selectors JSON response.
///
/// If the caller supplies an `if_none_match` value that matches the current
/// ETag, this returns `None` (the caller should send 304 Not Modified).
pub fn get_presets(if_none_match: Option<&str>) -> Option<(&'static str, &'static str)> {
    let payload = &*PRESETS;
    if let Some(inm) = if_none_match {
        if inm == payload.etag || inm.trim_matches('"') == payload.etag.trim_matches('"') {
            return None;
        }
    }
    Some((&payload.json, &payload.etag))
}

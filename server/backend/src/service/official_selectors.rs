use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use shared::OfficialSelectorsResponse;

/// Embedded official selectors (compiled into the binary from the repo root.
/// The server serves this payload verbatim, with an ETag
/// derived from the SHA-256 hash so clients can do conditional fetches.
static OFFICIAL_SELECTORS: Lazy<SelectorPayload> = Lazy::new(|| {
    let raw = include_str!("../../../../official_selectors.json");
    let parsed: serde_json::Value =
        serde_json::from_str(raw).expect("official_selectors.json is not valid JSON");
    let selectors_array = parsed
        .get("selectors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let version = parsed.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

    let hash = Sha256::digest(raw.as_bytes());
    let hex: String = hash[..16].iter().map(|b| format!("{b:02x}")).collect();
    let etag = format!("\"sha256-{hex}\"");

    let response = OfficialSelectorsResponse {
        version,
        etag: Some(etag.clone()),
        selectors: selectors_array,
    };
    let json =
        serde_json::to_string(&response).expect("failed to serialise official selectors response");

    SelectorPayload { json, etag }
});

struct SelectorPayload {
    json: String,
    etag: String,
}

/// Return the cached official selectors JSON response.
///
/// If the caller supplies an `if_none_match` value that matches the current
/// ETag, this returns `None` (the caller should send 304 Not Modified).
pub fn get_official_selectors(if_none_match: Option<&str>) -> Option<(&'static str, &'static str)> {
    let payload = &*OFFICIAL_SELECTORS;
    if let Some(inm) = if_none_match {
        if inm == payload.etag || inm.trim_matches('"') == payload.etag.trim_matches('"') {
            return None;
        }
    }
    Some((&payload.json, &payload.etag))
}

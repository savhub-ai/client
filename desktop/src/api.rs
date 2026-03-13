use reqwest::{Client, Method, Response, Url};
use savhub_local::registry::{DataSource, SkillEntry};
use savhub_shared::{PagedResponse, SkillListItem};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The API version this client accepts. Registry must return the same major
/// version in `/health` → `apiVersion`, otherwise the user is warned.
pub const CLIENT_API_VERSION: u32 = 1;

/// Result of comparing client and registry API versions.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiCompatibility {
    /// Not yet checked.
    Unknown,
    /// Versions are compatible (same major).
    Compatible { registry_version: u32 },
    /// Major version mismatch – client must be updated.
    Incompatible { registry_version: u32 },
}

#[derive(Clone)]
pub struct ApiClient {
    pub base: String,
    client: Client,
    pub token: Option<String>,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base: base.into(),
            client: Client::new(),
            token,
        }
    }

    pub fn v1_url(&self, path: &str) -> Result<Url, String> {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let base = self.base.trim_end_matches('/');
        // If base already ends with /api/v1, don't append it again
        let full = if base.ends_with("/api/v1") || base.ends_with("/api/v1/") {
            format!("{base}{path}")
        } else {
            format!("{base}/api/v1{path}")
        };
        Url::parse(&full).map_err(|e| format!("invalid API base URL: {e}"))
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::GET, url, None).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json(Method::POST, url, Some(body)).await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::POST, url, None).await
    }

    #[allow(dead_code)]
    pub async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::DELETE, url, None).await
    }

    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let url = self.v1_url(path)?;
        let response = self.send(Method::GET, url, None::<&()>).await?;
        if response.status().is_success() {
            response
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| e.to_string())
        } else {
            Err(parse_error(response).await)
        }
    }

    async fn request_json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T, String> {
        let response = self.send(method, url, body).await?;
        if response.status().is_success() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(parse_error(response).await)
        }
    }

    async fn send<B: Serialize>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<Response, String> {
        let mut request = self.client.request(method, url);
        if let Some(token) = self.token.as_deref().filter(|t| !t.is_empty()) {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        request
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))
    }
}

impl ApiClient {
    /// Check the registry `/health` endpoint and compare `apiVersion` with
    /// [`CLIENT_API_VERSION`]. Returns the compatibility status.
    pub async fn check_api_compatibility(&self) -> ApiCompatibility {
        let Ok(value) = self.get_json::<Value>("/health").await else {
            return ApiCompatibility::Unknown;
        };
        let registry_version = value
            .get("apiVersion")
            .and_then(Value::as_u64)
            .unwrap_or(CLIENT_API_VERSION as u64) as u32;

        if registry_version == CLIENT_API_VERSION {
            ApiCompatibility::Compatible { registry_version }
        } else {
            ApiCompatibility::Incompatible { registry_version }
        }
    }
}

impl ApiClient {
    /// Download the registry zip archive from GitHub and return (bytes, commit_sha).
    #[allow(dead_code)]
    pub async fn download_registry_zip(
        &self,
        github_repo: &str,
    ) -> Result<(Vec<u8>, String), String> {
        // Get latest commit SHA
        let api_url = format!("https://api.github.com/repos/{github_repo}/commits/main");
        let sha_response = self
            .client
            .get(&api_url)
            .header("User-Agent", "savhub-desktop")
            .send()
            .await
            .map_err(|e| format!("failed to check registry: {e}"))?;
        let sha_json: serde_json::Value = sha_response
            .json()
            .await
            .map_err(|e| format!("invalid response: {e}"))?;
        let commit_sha = sha_json
            .get("sha")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        // Download zipball
        let zip_url = format!("https://api.github.com/repos/{github_repo}/zipball/main");
        let zip_response = self
            .client
            .get(&zip_url)
            .header("User-Agent", "savhub-desktop")
            .send()
            .await
            .map_err(|e| format!("failed to download registry: {e}"))?;

        if !zip_response.status().is_success() {
            return Err(format!(
                "registry download failed: {}",
                zip_response.status()
            ));
        }

        let bytes = zip_response
            .bytes()
            .await
            .map_err(|e| format!("failed to read registry data: {e}"))?
            .to_vec();

        Ok((bytes, commit_sha))
    }
}

/// Convert a server API `SkillListItem` into our unified `SkillEntry`.
#[allow(dead_code)]
pub fn skill_list_item_to_entry(item: &SkillListItem) -> SkillEntry {
    let version = item
        .latest_version
        .as_ref()
        .map(|v| v.version.clone());
    SkillEntry {
        slug: item.slug.clone(),
        name: item.display_name.clone(),
        description: item.summary.clone(),
        version,
        status: "active".to_string(),
        license: String::new(),
        categories: Vec::new(),
        keywords: Vec::new(),
        source: None,
        stars: Some(item.stats.stars as u32),
        starred_by_me: None,
        downloads: Some(item.stats.downloads as u64),
        owner: Some(item.owner.handle.clone()),
        data_source: Some(DataSource::Remote),
    }
}

/// Fetch skills from the remote API as `SkillEntry` items.
/// Returns `(entries, next_cursor)` or an error.
#[allow(dead_code)]
pub async fn fetch_remote_skills(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    cursor: Option<&str>,
) -> Result<(Vec<SkillEntry>, Option<String>), String> {
    let mut url = client.v1_url("/skills")?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string());
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.query_pairs_mut().append_pair("q", q);
    }
    if let Some(c) = cursor {
        url.query_pairs_mut().append_pair("cursor", c);
    }
    let resp = client
        .get_json::<PagedResponse<SkillListItem>>(&format!(
            "/skills?limit={limit}{}{}",
            query.filter(|q| !q.is_empty()).map(|q| format!("&q={q}")).unwrap_or_default(),
            cursor.map(|c| format!("&cursor={c}")).unwrap_or_default(),
        ))
        .await?;
    let entries = resp.items.iter().map(skill_list_item_to_entry).collect();
    Ok((entries, resp.next_cursor))
}

async fn parse_error(response: Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if let Ok(payload) = serde_json::from_str::<Value>(&text) {
        if let Some(message) = payload.get("error").and_then(Value::as_str) {
            return format!("{}: {}", status.as_u16(), message);
        }
    }
    if text.trim().is_empty() {
        status.to_string()
    } else {
        format!("{}: {}", status.as_u16(), text)
    }
}

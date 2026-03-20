use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, Method, Response, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Clone)]
pub struct ApiClient {
    base: String,
    client: Client,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base: base.into(),
            client: Client::new(),
            token,
        }
    }

    pub fn v1_url(&self, path: &str) -> Result<Url> {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let base = self.base.trim_end_matches('/');
        let full = if base.ends_with("/api/v1") || base.ends_with("/api/v1/") {
            format!("{base}{path}")
        } else {
            format!("{base}/api/v1{path}")
        };
        Url::parse(&full).with_context(|| format!("invalid API base URL: {}", self.base))
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request_json::<(), T>(Method::GET, self.v1_url(path)?, None)
            .await
    }

    pub async fn get_json_url<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        self.request_json::<(), T>(Method::GET, url, None).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request_json(Method::POST, self.v1_url(path)?, Some(body))
            .await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request_json::<(), T>(Method::POST, self.v1_url(path)?, None)
            .await
    }

    pub async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request_json::<(), T>(Method::DELETE, self.v1_url(path)?, None)
            .await
    }

    pub async fn get_bytes_url(&self, url: Url) -> Result<Vec<u8>> {
        let response = self.send(Method::GET, url, None::<&()>).await?;
        if response.status().is_success() {
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(Into::into)
        } else {
            bail!(parse_error(response).await);
        }
    }

    async fn request_json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T> {
        let request_method = method.clone();
        let request_url = url.clone();
        let request_body = body.and_then(|value| serde_json::to_string(value).ok());
        let response = self.send(method, url, body).await?;
        if response.status().is_success() {
            let status = response.status();
            let final_url = response.url().clone();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unknown>")
                .to_string();
            let text = response.text().await.map_err(|error| {
                println!(
                    "error reading response body\nmethod: {request_method}\nurl: {request_url}\nfinal_url: {final_url}\nstatus: {status}\ncontent_type: {content_type}\nrequest_body: {}\nerror: {error}",
                    request_body.as_deref().unwrap_or("<none>")
                );
                anyhow!("error reading response body: {error}")
            })?;
            serde_json::from_str::<T>(&text).map_err(|error| {
                println!(
                    "error decoding response body\nmethod: {request_method}\nurl: {request_url}\nfinal_url: {final_url}\nstatus: {status}\ncontent_type: {content_type}\nexpected_type: {}\nrequest_body: {}\nresponse_body: {}",
                    std::any::type_name::<T>(),
                    request_body.as_deref().unwrap_or("<none>"),
                    response_preview(&text)
                );
                anyhow!("error decoding response body: {error}")
            })
        } else {
            bail!(parse_error(response).await);
        }
    }

    async fn send<B: Serialize>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<Response> {
        let mut request = self.client.request(method, url);
        if let Some(token) = self.token.as_deref().filter(|token| !token.is_empty()) {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        request
            .send()
            .await
            .map_err(|error| anyhow!("request failed: {error}"))
    }
}

impl ApiClient {
    /// Download the registry zip from GitHub. Returns (zip_bytes, commit_sha).
    #[allow(dead_code)]
    pub async fn download_registry_zip(&self, github_repo: &str) -> Result<(Vec<u8>, String)> {
        let sha_url = format!("https://api.github.com/repos/{github_repo}/commits/main");
        let sha_resp = self
            .client
            .get(&sha_url)
            .header("User-Agent", "savhub-cli")
            .send()
            .await?;
        let sha_json: Value = sha_resp.json().await?;
        let commit_sha = sha_json
            .get("sha")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let zip_url = format!("https://api.github.com/repos/{github_repo}/zipball/main");
        let zip_resp = self
            .client
            .get(&zip_url)
            .header("User-Agent", "savhub-cli")
            .send()
            .await?;
        if !zip_resp.status().is_success() {
            bail!("registry download failed: {}", zip_resp.status());
        }
        let bytes = zip_resp.bytes().await?.to_vec();
        Ok((bytes, commit_sha))
    }
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

fn response_preview(text: &str) -> String {
    const LIMIT: usize = 4096;
    let truncated: String = text.chars().take(LIMIT).collect();
    if text.chars().count() > LIMIT {
        format!("{truncated}...(truncated)")
    } else {
        truncated
    }
}

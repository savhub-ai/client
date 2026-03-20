#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use reqwest::{Client, Method, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub async fn get_json<T: DeserializeOwned>(
    api_base: &str,
    token: Option<&str>,
    path: &str,
) -> Result<T, String> {
    request_json::<(), T>(Method::GET, api_base, token, path, None).await
}

pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    api_base: &str,
    token: Option<&str>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    request_json(Method::POST, api_base, token, path, Some(body)).await
}

pub async fn post_empty<T: DeserializeOwned>(
    api_base: &str,
    token: Option<&str>,
    path: &str,
) -> Result<T, String> {
    let client = Client::new();
    let mut request = client.post(format!("{}{}", api_base.trim_end_matches('/'), path));
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    parse_json_response(response).await
}

#[allow(dead_code)]
pub async fn put_json<B: Serialize, T: DeserializeOwned>(
    api_base: &str,
    token: Option<&str>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    request_json(Method::PUT, api_base, token, path, Some(body)).await
}

#[allow(dead_code)]
pub async fn delete_json<T: DeserializeOwned>(
    api_base: &str,
    token: Option<&str>,
    path: &str,
) -> Result<T, String> {
    let client = Client::new();
    let mut request = client.delete(format!("{}{}", api_base.trim_end_matches('/'), path));
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    parse_json_response(response).await
}

async fn request_json<B: Serialize, T: DeserializeOwned>(
    method: Method,
    api_base: &str,
    token: Option<&str>,
    path: &str,
    body: Option<&B>,
) -> Result<T, String> {
    let client = Client::new();
    let mut request = client.request(
        method,
        format!("{}{}", api_base.trim_end_matches('/'), path),
    );
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    parse_json_response(response).await
}

async fn parse_json_response<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    if response.status().is_success() {
        response
            .json::<T>()
            .await
            .map_err(|error| error.to_string())
    } else {
        Err(parse_error(response).await)
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

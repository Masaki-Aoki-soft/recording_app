//! Clerk Frontend API へのリクエストを Rust 側から送るプロキシ
//!
//! Clerk をネイティブモード（Authorization ヘッダでクライアントを識別）で使う場合、
//! FAPI は `Origin` と `Authorization` の両方を持つリクエストを拒否する。
//! WebView の fetch は必ず `Origin` を付けてしまうため、Clerk へのリクエストだけは
//! Rust（reqwest）から送り、モバイルアプリと同じ形（Origin 無し）にする。
//!
//! 送信先はビルド時に埋め込んだ Publishable key から求めた FAPI のホストに限定する。

use std::sync::OnceLock;

use base64::Engine;
use serde::{Deserialize, Serialize};

/// ビルド時に .env.local / 環境変数から埋め込まれる Publishable key
const CLERK_PUBLISHABLE_KEY: Option<&str> = option_env!("CLERK_PUBLISHABLE_KEY");

/// WebView から転送しない（またはできない）ヘッダ
const SKIPPED_REQUEST_HEADERS: &[&str] = &["origin", "referer", "cookie", "host", "content-length"];

#[derive(Debug, Deserialize)]
pub struct ProxyRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProxyResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// Publishable key（pk_test_xxx / pk_live_xxx）から FAPI のホスト名を取り出す
fn frontend_api_host(publishable_key: &str) -> Option<String> {
    let encoded = publishable_key
        .strip_prefix("pk_test_")
        .or_else(|| publishable_key.strip_prefix("pk_live_"))?;
    // パディング有無のどちらにも対応
    let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(encoded.trim_end_matches('='))
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let host = text.trim_end_matches('$').trim();
    (!host.is_empty()).then(|| host.to_string())
}

fn allowed_host() -> Result<&'static str, String> {
    static HOST: OnceLock<Option<String>> = OnceLock::new();
    HOST.get_or_init(|| CLERK_PUBLISHABLE_KEY.and_then(frontend_api_host))
        .as_deref()
        .ok_or_else(|| {
            "Clerk の Publishable key がアプリに埋め込まれていません（.env.local の NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY を設定して再ビルドしてください）"
                .to_string()
        })
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("MeetingRec/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client")
    })
}

/// 送信先 URL を検証する（https かつ Clerk FAPI のホストのみ許可）
fn validate_url(url: &str, host: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "不正な URL です".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str() != Some(host) {
        return Err("Clerk 以外への通信は許可されていません".into());
    }
    Ok(parsed)
}

#[tauri::command]
pub async fn clerk_fetch(request: ProxyRequest) -> Result<ProxyResponse, String> {
    let host = allowed_host()?;
    let url = validate_url(&request.url, host)?;
    let method = reqwest::Method::from_bytes(request.method.to_ascii_uppercase().as_bytes())
        .map_err(|_| "不正な HTTP メソッドです".to_string())?;

    let mut builder = client().request(method, url);
    for (name, value) in &request.headers {
        if !SKIPPED_REQUEST_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            builder = builder.header(name, value);
        }
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("認証サーバーに接続できません: {}", e))?;

    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();
    let body = response
        .text()
        .await
        .map_err(|e| format!("認証サーバーの応答を読み取れません: {}", e))?;

    Ok(ProxyResponse {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_frontend_api_host_from_publishable_key() {
        // "clerk.example.com$" を base64 した値
        assert_eq!(
            frontend_api_host("pk_test_Y2xlcmsuZXhhbXBsZS5jb20k").as_deref(),
            Some("clerk.example.com")
        );
        assert_eq!(
            frontend_api_host("pk_live_Y2xlcmsuZXhhbXBsZS5jb20k").as_deref(),
            Some("clerk.example.com")
        );
        assert_eq!(frontend_api_host("sk_test_abc"), None);
    }

    #[test]
    fn only_allows_https_requests_to_the_frontend_api() {
        let host = "clerk.example.com";
        assert!(validate_url("https://clerk.example.com/v1/client?_is_native=1", host).is_ok());
        assert!(validate_url("http://clerk.example.com/v1/client", host).is_err());
        assert!(validate_url("https://evil.example.com/v1/client", host).is_err());
        assert!(validate_url("https://clerk.example.com.evil.com/", host).is_err());
        assert!(validate_url("not a url", host).is_err());
    }
}

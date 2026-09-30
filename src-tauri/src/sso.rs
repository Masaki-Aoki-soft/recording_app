//! Clerk のソーシャルログイン（Google SSO）用のループバック受信
//!
//! Google は WebView 内での OAuth をブロックするため、認証はシステムブラウザで行う。
//! Clerk は認証完了後に `redirectUrl`（ここで待ち受ける http://127.0.0.1:<PORT>/sso-callback）へ
//! `rotating_token_nonce` 付きでリダイレクトするので、その URL をフロントへ返す。
//! フロントは nonce を使って `signIn.reload({ rotatingTokenNonce })` でセッションを確定させる。

use std::time::Duration;

use log::info;
use tauri::{AppHandle, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, Notify};

/// Clerk ダッシュボードの「Allowlist for mobile SSO redirect」に登録する URL のポート
const SSO_CALLBACK_PORT: u16 = 47615;
const SSO_CALLBACK_PATH: &str = "/sso-callback";
const SSO_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct SsoState {
    cancel: Notify,
    in_progress: Mutex<()>,
}

fn redirect_url() -> String {
    format!("http://127.0.0.1:{}{}", SSO_CALLBACK_PORT, SSO_CALLBACK_PATH)
}

/// signIn.create に渡す redirectUrl
#[tauri::command]
pub fn sso_redirect_url() -> String {
    redirect_url()
}

/// 認証 URL をシステムブラウザで開き、Clerk からのコールバック URL（クエリ付き）を返す
#[tauri::command]
pub async fn wait_for_sso_callback(
    app: AppHandle,
    state: State<'_, SsoState>,
    auth_url: String,
) -> Result<String, String> {
    if !auth_url.starts_with("https://") {
        return Err("不正な認証 URL です".into());
    }
    let _guard = state
        .in_progress
        .try_lock()
        .map_err(|_| "別のログイン処理が進行中です".to_string())?;

    // ブラウザを開く前に待ち受けを開始する
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", SSO_CALLBACK_PORT))
        .await
        .map_err(|e| {
            format!(
                "ログイン用のポート {} を使用できません（他のアプリが使用中の可能性があります）: {}",
                SSO_CALLBACK_PORT, e
            )
        })?;

    tauri_plugin_opener::open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("ブラウザを開けません: {}", e))?;
    info!("Waiting for SSO callback on {}", redirect_url());

    let result = tokio::select! {
        r = tokio::time::timeout(SSO_TIMEOUT, accept_callback(listener)) => {
            r.map_err(|_| "ログインがタイムアウトしました".to_string())?
        }
        _ = state.cancel.notified() => Err("ログインをキャンセルしました".to_string()),
    };

    // 認証後はアプリのウィンドウを前面に戻す
    crate::show_main_window(&app);
    result
}

/// 進行中の SSO 待ち受けをキャンセル
#[tauri::command]
pub fn cancel_sso(state: State<'_, SsoState>) {
    state.cancel.notify_waiters();
}

/// /sso-callback へのリクエストを受け取り、完全な URL を返す（favicon 等は読み飛ばす）
async fn accept_callback(listener: tokio::net::TcpListener) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("コールバックを受信できません: {}", e))?;

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let Some(path) = request_path(&request) else {
            continue;
        };

        if path != SSO_CALLBACK_PATH && !path.starts_with(&format!("{}?", SSO_CALLBACK_PATH)) {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            continue;
        }

        let body = "<html><head><meta charset=\"utf-8\"><title>MeetingRec</title></head>\
            <body style=\"font-family:sans-serif;text-align:center;padding-top:3em\">\
            <h2>MeetingRec</h2><p>ログイン処理が完了しました。このタブを閉じて MeetingRec に戻ってください。</p>\
            <script>setTimeout(function(){window.close()},500)</script></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;

        return Ok(format!("http://127.0.0.1:{}{}", SSO_CALLBACK_PORT, path));
    }
}

/// HTTP リクエスト行からパス（クエリ含む）を取り出す
fn request_path(request: &str) -> Option<&str> {
    let mut parts = request.lines().next()?.split_whitespace();
    (parts.next()? == "GET").then_some(())?;
    parts.next()
}

#[cfg(test)]
mod tests {
    use super::request_path;

    #[test]
    fn extracts_request_path() {
        let req = "GET /sso-callback?rotating_token_nonce=abc HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(request_path(req), Some("/sso-callback?rotating_token_nonce=abc"));
        assert_eq!(request_path("POST / HTTP/1.1\r\n"), None);
        assert_eq!(request_path(""), None);
    }
}

//! Clerk のソーシャルログイン（Google SSO）のコールバック受信
//!
//! Google は WebView 内での OAuth をブロックするため、認証はシステムブラウザで行う。
//! Clerk は認証完了後に `redirectUrl`（カスタムスキーム `meetingrec://sso-callback`）へ
//! `rotating_token_nonce` 付きでリダイレクトする。Windows はそれを新しいプロセスの引数として
//! 起動するが、single-instance + deep-link プラグインが既存のプロセスへ転送するので、
//! ここで受け取ってフロントへ返す。フロントは nonce で `signIn.reload({ rotatingTokenNonce })` を行う。
//!
//! Clerk はセキュリティ上、Clerk ダッシュボードの「Allowlist for mobile SSO redirect」に
//! 登録された URL にしか nonce を付けない。

use std::sync::Mutex as StdMutex;
use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, State, Url};
use tokio::sync::{oneshot, Mutex, Notify};

/// Clerk ダッシュボードの許可リストに登録する URL
const SSO_SCHEME: &str = "meetingrec";
const SSO_HOST: &str = "sso-callback";
const SSO_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct SsoState {
    /// コールバック待ちの送信口（待ち受け中のみ Some）
    pending: StdMutex<Option<oneshot::Sender<String>>>,
    cancel: Notify,
    in_progress: Mutex<()>,
}

fn redirect_url() -> String {
    format!("{}://{}", SSO_SCHEME, SSO_HOST)
}

/// deep link（meetingrec://...）を受け取ったときに呼ばれる
pub fn handle_deep_link(state: &SsoState, url: &Url) {
    if url.scheme() != SSO_SCHEME || url.host_str() != Some(SSO_HOST) {
        warn!("Ignoring unexpected deep link: {}://{:?}", url.scheme(), url.host_str());
        return;
    }
    match state.pending.lock().unwrap().take() {
        Some(tx) => {
            let _ = tx.send(url.to_string());
        }
        None => warn!("Received SSO callback but no login is in progress"),
    }
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

    // ブラウザを開く前に受け口を用意する
    let (tx, rx) = oneshot::channel::<String>();
    *state.pending.lock().unwrap() = Some(tx);

    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        state.pending.lock().unwrap().take();
        return Err(format!("ブラウザを開けません: {}", e));
    }
    info!("Waiting for SSO callback on {}", redirect_url());

    let result = tokio::select! {
        r = tokio::time::timeout(SSO_TIMEOUT, rx) => match r {
            Ok(Ok(url)) => Ok(url),
            Ok(Err(_)) => Err("ログイン結果を受け取れませんでした".to_string()),
            Err(_) => Err("ログインがタイムアウトしました".to_string()),
        },
        _ = state.cancel.notified() => Err("ログインをキャンセルしました".to_string()),
    };
    state.pending.lock().unwrap().take();

    // 認証後はアプリのウィンドウを前面に戻す
    crate::show_main_window(&app);
    result
}

/// 進行中の SSO 待ち受けをキャンセル
#[tauri::command]
pub fn cancel_sso(state: State<'_, SsoState>) {
    state.cancel.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delivers_matching_deep_link_to_pending_waiter() {
        let state = SsoState::default();
        let (tx, rx) = oneshot::channel();
        *state.pending.lock().unwrap() = Some(tx);

        // 関係ない URL は無視される
        handle_deep_link(&state, &Url::parse("meetingrec://other?x=1").unwrap());
        assert!(state.pending.lock().unwrap().is_some());

        let url = Url::parse("meetingrec://sso-callback?rotating_token_nonce=abc").unwrap();
        handle_deep_link(&state, &url);
        assert_eq!(rx.await.unwrap(), "meetingrec://sso-callback?rotating_token_nonce=abc");
        assert!(state.pending.lock().unwrap().is_none());
    }

    #[test]
    fn redirect_url_uses_custom_scheme() {
        assert_eq!(redirect_url(), "meetingrec://sso-callback");
    }
}

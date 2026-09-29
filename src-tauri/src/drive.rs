//! Google Drive 連携（OAuth2 PKCE + レジューマブルアップロード + 再試行キュー）

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use log::{error, info, warn};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::models::{DriveAuthStatus, DriveConfig, UploadProgressPayload};
use crate::settings;

/// Google OAuth2 のエンドポイント
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";

/// スコープ
const SCOPES: &str =
    "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/userinfo.email";

/// Google Client ID / Secret（ビルド時に src-tauri/.env または環境変数から埋め込み）。
/// デスクトップアプリの client_secret は機密扱いされない（Google の仕様）。
const GOOGLE_CLIENT_ID: Option<&str> = option_env!("GOOGLE_CLIENT_ID");
const GOOGLE_CLIENT_SECRET: Option<&str> = option_env!("GOOGLE_CLIENT_SECRET");

/// トークンは Windows 資格情報マネージャーに保存する
const KEYRING_SERVICE: &str = "com.meetingrec.app";
const ACCESS_TOKEN_KEY: &str = "google_access_token";
const REFRESH_TOKEN_KEY: &str = "google_refresh_token";
const TOKEN_EXPIRY_KEY: &str = "google_token_expiry";

/// settings.json のキー
const EMAIL_KEY: &str = "google_email";
const PENDING_UPLOADS_KEY: &str = "pending_uploads";
const UPLOADED_FILES_KEY: &str = "uploaded_files";

/// レジューマブルアップロードのチャンクサイズ（256KiB の倍数である必要がある）
const CHUNK_SIZE: u64 = 8 * 1024 * 1024;
const MAX_RETRIES: u32 = 5;

/// アップロードキューの同時実行を防ぐロック
pub struct UploadLock(pub tokio::sync::Mutex<()>);

// =====================================================
// トークン保存（keyring）
// =====================================================

fn secret_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, key).map_err(|e| format!("資格情報ストアを開けません: {}", e))
}

fn secret_save(key: &str, value: &str) -> Result<(), String> {
    secret_entry(key)?
        .set_password(value)
        .map_err(|e| format!("トークンを保存できません: {}", e))
}

fn secret_load(key: &str) -> Option<String> {
    secret_entry(key).ok()?.get_password().ok()
}

fn secret_delete(key: &str) {
    if let Ok(entry) = secret_entry(key) {
        let _ = entry.delete_credential();
    }
}

/// 旧バージョンが平文 tokens.json に保存していたトークンを keyring へ移行する
pub fn migrate_legacy_tokens(app: &AppHandle) {
    let Ok(store) = app.store("tokens.json") else {
        return;
    };
    let mut migrated = false;
    for key in [ACCESS_TOKEN_KEY, REFRESH_TOKEN_KEY, TOKEN_EXPIRY_KEY] {
        if let Some(value) = store.get(key).and_then(|v| v.as_str().map(|s| s.to_string())) {
            if secret_load(key).is_none() {
                let _ = secret_save(key, &value);
            }
            store.delete(key);
            migrated = true;
        }
    }
    if migrated {
        let _ = store.save();
        info!("Migrated Google tokens to the OS credential store");
    }
}

fn client_credentials() -> Result<(&'static str, &'static str), String> {
    match (GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET) {
        (Some(id), Some(secret)) if !id.is_empty() => Ok((id, secret)),
        _ => Err(
            "Google の Client ID が設定されていません（src-tauri/.env の GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET を設定してビルドしてください）"
                .into(),
        ),
    }
}

// =====================================================
// OAuth2
// =====================================================

/// トークンレスポンス
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    email: String,
}

/// PKCE code_verifier（UUID v4 は OS の CSPRNG を使う。hex 64 文字は RFC 7636 の条件を満たす）
fn generate_code_verifier() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// PKCE code_challenge（S256）
fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// OAuth2 PKCE 認証フローを実行（システムブラウザで同意 → ループバックで受け取り）
pub async fn start_oauth(app: &AppHandle) -> Result<(), String> {
    let (client_id, client_secret) = client_credentials()?;

    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = uuid::Uuid::new_v4().simple().to_string();

    // ローカルリダイレクトサーバーを起動
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("ローカルサーバーを起動できません: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("ローカルアドレスを取得できません: {}", e))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{}", port);

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        AUTH_URL,
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPES),
        urlencoding::encode(&code_challenge),
        state,
    );

    tauri_plugin_opener::open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("ブラウザを開けません: {}", e))?;

    // コールバックを待つ（タイムアウト: 5分）
    let auth_code = tokio::time::timeout(
        Duration::from_secs(300),
        wait_for_auth_code(listener, &state),
    )
    .await
    .map_err(|_| "認証がタイムアウトしました".to_string())??;

    let client = Client::new();
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", auth_code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("トークン取得に失敗しました: {}", e))?;
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("トークン取得に失敗しました: {}", body));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|e| format!("トークンを解析できません: {}", e))?;

    save_token(&token)?;
    if let Some(refresh) = &token.refresh_token {
        secret_save(REFRESH_TOKEN_KEY, refresh)?;
    }

    // メールアドレスを取得して保存（表示用）
    let email = client
        .get(USERINFO_URL)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .ok()
        .filter(|r| r.status().is_success());
    if let Some(resp) = email {
        if let Ok(info) = resp.json::<UserInfoResponse>().await {
            let _ = settings::save(app, EMAIL_KEY, &info.email);
        }
    }

    info!("Google Drive connected");
    Ok(())
}

fn save_token(token: &TokenResponse) -> Result<(), String> {
    secret_save(ACCESS_TOKEN_KEY, &token.access_token)?;
    let expiry = chrono::Local::now() + chrono::Duration::seconds(token.expires_in as i64);
    secret_save(TOKEN_EXPIRY_KEY, &expiry.to_rfc3339())
}

/// 認証コードを待つ（ローカル HTTP サーバー）。favicon などの無関係なリクエストは読み飛ばす
async fn wait_for_auth_code(
    listener: tokio::net::TcpListener,
    expected_state: &str,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("コールバックを受信できません: {}", e))?;

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);

        let code = extract_query_param(&request, "code");
        let error = extract_query_param(&request, "error");
        if code.is_none() && error.is_none() {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }

        let (ok, message) = match (&code, &error) {
            (Some(_), _) if extract_query_param(&request, "state").as_deref() != Some(expected_state) => {
                (false, "不正なリクエストです（state 不一致）")
            }
            (Some(_), _) => (true, "認証に成功しました。このタブを閉じて MeetingRec に戻ってください。"),
            _ => (false, "認証がキャンセルされました。MeetingRec に戻ってやり直してください。"),
        };
        let body = format!(
            "<html><head><meta charset=\"utf-8\"></head><body style=\"font-family:sans-serif;text-align:center;padding-top:3em\"><h2>MeetingRec</h2><p>{}</p></body></html>",
            message
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;

        return if ok {
            Ok(code.unwrap_or_default())
        } else {
            Err(message.to_string())
        };
    }
}

/// HTTP リクエスト行からクエリパラメータを抽出
fn extract_query_param(request: &str, param: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;

    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == param).then(|| {
            urlencoding::decode(value)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| value.to_string())
        })
    })
}

/// アクセストークンを取得（期限切れならリフレッシュ）
async fn get_valid_access_token() -> Result<String, String> {
    if let (Some(token), Some(expiry)) = (secret_load(ACCESS_TOKEN_KEY), secret_load(TOKEN_EXPIRY_KEY)) {
        if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(&expiry) {
            if chrono::Local::now() < expiry.with_timezone(&chrono::Local) - chrono::Duration::minutes(5) {
                return Ok(token);
            }
        }
    }
    refresh_access_token().await
}

async fn refresh_access_token() -> Result<String, String> {
    let (client_id, client_secret) = client_credentials()?;
    let refresh_token = secret_load(REFRESH_TOKEN_KEY)
        .ok_or("Google Drive が連携されていません".to_string())?;

    let response = Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("トークン更新に失敗しました: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status == StatusCode::BAD_REQUEST && body.contains("invalid_grant") {
            // 連携が取り消された / 期限切れ
            secret_delete(ACCESS_TOKEN_KEY);
            secret_delete(REFRESH_TOKEN_KEY);
            secret_delete(TOKEN_EXPIRY_KEY);
            return Err("Google Drive の連携が無効になりました。再度連携してください".into());
        }
        return Err(format!("トークン更新に失敗しました: {}", body));
    }

    let token: TokenResponse = response
        .json()
        .await
        .map_err(|e| format!("トークンを解析できません: {}", e))?;
    save_token(&token)?;
    Ok(token.access_token)
}

pub fn is_connected(_app: &AppHandle) -> bool {
    secret_load(REFRESH_TOKEN_KEY).is_some()
}

pub fn auth_status(app: &AppHandle) -> DriveAuthStatus {
    let connected = is_connected(app);
    let email: Option<String> = settings::load(app, EMAIL_KEY);
    DriveAuthStatus {
        connected,
        email: if connected { email } else { None },
    }
}

/// 連携を解除（トークンを失効させて削除）
pub async fn disconnect(app: &AppHandle) -> Result<(), String> {
    if let Some(token) = secret_load(REFRESH_TOKEN_KEY).or_else(|| secret_load(ACCESS_TOKEN_KEY)) {
        let _ = Client::new()
            .post(REVOKE_URL)
            .form(&[("token", token.as_str())])
            .send()
            .await;
    }
    secret_delete(ACCESS_TOKEN_KEY);
    secret_delete(REFRESH_TOKEN_KEY);
    secret_delete(TOKEN_EXPIRY_KEY);
    settings::save(app, EMAIL_KEY, &Option::<String>::None)?;
    Ok(())
}

// =====================================================
// アップロードキュー
// =====================================================

fn load_list(app: &AppHandle, key: &str) -> Vec<String> {
    settings::load(app, key)
}

/// アップロード済みのファイル名一覧
pub fn uploaded_files(app: &AppHandle) -> Vec<String> {
    load_list(app, UPLOADED_FILES_KEY)
}

/// ファイルをアップロード待ちキューに追加
pub fn enqueue(app: &AppHandle, path: &Path) -> Result<(), String> {
    let mut pending = load_list(app, PENDING_UPLOADS_KEY);
    let path = path.to_string_lossy().into_owned();
    if !pending.contains(&path) {
        pending.push(path);
        settings::save(app, PENDING_UPLOADS_KEY, &pending)?;
    }
    Ok(())
}

/// キュー内のファイルを順番にアップロードする。最初に失敗したエラーを返す
pub async fn process_queue(app: &AppHandle) -> Result<(), String> {
    let lock = app.state::<UploadLock>();
    let _guard = lock.0.lock().await;

    if !is_connected(app) {
        return Err("Google Drive が連携されていません".into());
    }

    let config: DriveConfig = settings::load(app, settings::KEY_DRIVE_CONFIG);
    let mut first_error = None;

    for path_str in load_list(app, PENDING_UPLOADS_KEY) {
        let path = PathBuf::from(&path_str);
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let finish = |app: &AppHandle| {
            let mut pending = load_list(app, PENDING_UPLOADS_KEY);
            pending.retain(|p| p != &path_str);
            let _ = settings::save(app, PENDING_UPLOADS_KEY, &pending);
        };

        if !path.exists() {
            warn!("Pending upload no longer exists: {}", path_str);
            finish(app);
            continue;
        }

        match upload_file(app, &path, &file_name, &config.folder_name).await {
            Ok(()) => {
                finish(app);
                let mut uploaded = load_list(app, UPLOADED_FILES_KEY);
                if !uploaded.contains(&file_name) {
                    uploaded.push(file_name.clone());
                    let _ = settings::save(app, UPLOADED_FILES_KEY, &uploaded);
                }
                if config.delete_after_upload {
                    if let Err(e) = std::fs::remove_file(&path) {
                        warn!("Failed to delete uploaded file {}: {}", path_str, e);
                    }
                }
            }
            Err(e) => {
                error!("Upload failed for {}: {}", file_name, e);
                emit_progress(app, &file_name, 0.0, "error", Some(e.clone()));
                first_error.get_or_insert(e);
            }
        }
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// 起動時と定期的に、未アップロードのファイルを再試行する
pub async fn retry_loop(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        if is_connected(&app) && !load_list(&app, PENDING_UPLOADS_KEY).is_empty() {
            if let Err(e) = process_queue(&app).await {
                warn!("Pending uploads remain: {}", e);
            }
        }
        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
    }
}

fn emit_progress(app: &AppHandle, file_name: &str, percent: f64, status: &str, message: Option<String>) {
    let _ = app.emit(
        "upload-progress",
        UploadProgressPayload {
            file_name: file_name.to_string(),
            progress_percent: percent,
            status: status.to_string(),
            message,
        },
    );
}

// =====================================================
// Drive API
// =====================================================

/// Drive 検索クエリの文字列リテラル用エスケープ
fn escape_query_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

/// レジューマブルアップロードでファイルをアップロード
async fn upload_file(app: &AppHandle, path: &Path, file_name: &str, folder_name: &str) -> Result<(), String> {
    let client = Client::new();
    let token = get_valid_access_token().await?;
    let folder_id = find_or_create_folder(&client, &token, folder_name).await?;

    let total = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("ファイルを読めません: {}", e))?
        .len();

    if total == 0 {
        return Err("空のファイルはアップロードできません".into());
    }
    emit_progress(app, file_name, 0.0, "uploading", None);

    // 1. アップロードセッションを作成
    let metadata = serde_json::json!({ "name": file_name, "parents": [folder_id] });
    let response = client
        .post(format!("{}?uploadType=resumable", DRIVE_UPLOAD_URL))
        .bearer_auth(&token)
        .header("X-Upload-Content-Type", "video/mp4")
        .header("X-Upload-Content-Length", total.to_string())
        .json(&metadata)
        .send()
        .await
        .map_err(|e| format!("アップロードを開始できません: {}", e))?;
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("アップロードを開始できません: {}", body));
    }
    let session_url = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or("アップロード URL を取得できません")?
        .to_string();

    // 2. チャンク単位で送信
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("ファイルを開けません: {}", e))?;
    let mut offset: u64 = 0;
    let mut retries = 0;
    let mut last_percent = -1.0;

    while offset < total {
        let len = CHUNK_SIZE.min(total - offset);
        let mut chunk = vec![0u8; len as usize];
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| e.to_string())?;
        file.read_exact(&mut chunk)
            .await
            .map_err(|e| format!("ファイルを読めません: {}", e))?;

        let range = format!("bytes {}-{}/{}", offset, offset + len - 1, total);
        let result = client
            .put(&session_url)
            .header(reqwest::header::CONTENT_RANGE, range)
            .body(chunk)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                emit_progress(app, file_name, 100.0, "completed", None);
                info!("Uploaded '{}' to Google Drive", file_name);
                return Ok(());
            }
            Ok(resp) if resp.status().as_u16() == 308 => {
                offset = next_offset_from_range(resp.headers()).unwrap_or(offset + len);
                retries = 0;
                let percent = (offset as f64 / total as f64 * 100.0).floor();
                if percent > last_percent {
                    emit_progress(app, file_name, percent, "uploading", None);
                    last_percent = percent;
                }
            }
            Ok(resp) if resp.status().is_server_error() && retries < MAX_RETRIES => {
                retries += 1;
                warn!("Upload chunk failed ({}), retry {}", resp.status(), retries);
                tokio::time::sleep(Duration::from_secs(2u64.pow(retries))).await;
                offset = query_upload_offset(&client, &session_url, total).await.unwrap_or(offset);
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("アップロードに失敗しました ({}): {}", status, body));
            }
            Err(e) if retries < MAX_RETRIES => {
                retries += 1;
                warn!("Upload chunk network error: {}, retry {}", e, retries);
                tokio::time::sleep(Duration::from_secs(2u64.pow(retries))).await;
                offset = query_upload_offset(&client, &session_url, total).await.unwrap_or(offset);
            }
            Err(e) => return Err(format!("アップロードに失敗しました: {}", e)),
        }
    }
    Err("アップロードが完了しませんでした".into())
}

/// 308 応答の Range ヘッダ（bytes=0-N）から次に送るべき位置を得る
fn next_offset_from_range(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let range = headers.get(reqwest::header::RANGE)?.to_str().ok()?;
    let end = range.rsplit('-').next()?.parse::<u64>().ok()?;
    Some(end + 1)
}

/// 中断したアップロードのサーバー側の受信済みバイト数を問い合わせる
async fn query_upload_offset(client: &Client, session_url: &str, total: u64) -> Option<u64> {
    let resp = client
        .put(session_url)
        .header(reqwest::header::CONTENT_RANGE, format!("bytes */{}", total))
        .send()
        .await
        .ok()?;
    if resp.status().as_u16() == 308 {
        Some(next_offset_from_range(resp.headers()).unwrap_or(0))
    } else {
        None
    }
}

/// Google Drive でフォルダを検索 or 作成
async fn find_or_create_folder(client: &Client, token: &str, folder_name: &str) -> Result<String, String> {
    let query = format!(
        "name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false",
        escape_query_value(folder_name)
    );

    #[derive(Deserialize)]
    struct FileList {
        files: Vec<FileInfo>,
    }
    #[derive(Deserialize)]
    struct FileInfo {
        id: String,
    }

    let response = client
        .get(DRIVE_FILES_URL)
        .bearer_auth(token)
        .query(&[("q", query.as_str()), ("fields", "files(id,name)")])
        .send()
        .await
        .map_err(|e| format!("フォルダを検索できません: {}", e))?;

    if response.status().is_success() {
        let list: FileList = response
            .json()
            .await
            .map_err(|e| format!("フォルダ検索結果を解析できません: {}", e))?;
        if let Some(folder) = list.files.first() {
            return Ok(folder.id.clone());
        }
    }

    let response = client
        .post(DRIVE_FILES_URL)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "name": folder_name,
            "mimeType": "application/vnd.google-apps.folder"
        }))
        .send()
        .await
        .map_err(|e| format!("フォルダを作成できません: {}", e))?;

    if response.status().is_success() {
        let folder: FileInfo = response
            .json()
            .await
            .map_err(|e| format!("フォルダ作成結果を解析できません: {}", e))?;
        Ok(folder.id)
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("フォルダを作成できません: {}", body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_drive_query_values() {
        assert_eq!(escape_query_value("Bob's \\ files"), "Bob\\'s \\\\ files");
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        // RFC 7636 Appendix B
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            generate_code_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGKSsnh5cM"
        );
        let v = generate_code_verifier();
        assert!(v.len() >= 43 && v.len() <= 128);
    }

    #[test]
    fn extracts_callback_params() {
        let req = "GET /?state=abc&code=4%2F0Ab HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(extract_query_param(req, "code").as_deref(), Some("4/0Ab"));
        assert_eq!(extract_query_param(req, "state").as_deref(), Some("abc"));
        assert_eq!(extract_query_param(req, "error"), None);
    }
}

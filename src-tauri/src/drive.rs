use log::info;
use reqwest::Client;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::models::{AuthStatus, UploadProgressPayload};

/// Store に保存するキー名
const ACCESS_TOKEN_KEY: &str = "google_access_token";
const REFRESH_TOKEN_KEY: &str = "google_refresh_token";
const TOKEN_EXPIRY_KEY: &str = "google_token_expiry";

/// Google OAuth2 のエンドポイント
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";

/// スコープ
const SCOPES: &str = "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/userinfo.email";

/// Google Client ID（ビルド時に環境変数から取得、もしくはデフォルト値）
fn get_client_id() -> String {
    option_env!("GOOGLE_CLIENT_ID")
        .unwrap_or("")
        .to_string()
}

/// トークンレスポンス
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    #[allow(dead_code)]
    token_type: String,
}

/// UserInfo レスポンス
#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    email: String,
}

/// Store にトークンを保存
fn token_save(app: &AppHandle, key: &str, value: &str) -> Result<(), String> {
    let store = app
        .store("tokens.json")
        .map_err(|e| format!("Failed to open token store: {}", e))?;
    store.set(key, serde_json::Value::String(value.to_string()));
    store
        .save()
        .map_err(|e| format!("Failed to save token store: {}", e))?;
    Ok(())
}

/// Store からトークンを読み込み
fn token_load(app: &AppHandle, key: &str) -> Result<Option<String>, String> {
    let store = app
        .store("tokens.json")
        .map_err(|e| format!("Failed to open token store: {}", e))?;
    Ok(store
        .get(key)
        .and_then(|v| v.as_str().map(|s| s.to_string())))
}

/// OAuth2 PKCE 認証フローを開始
pub async fn start_oauth(app: &AppHandle) -> Result<String, String> {
    let client_id = get_client_id();
    if client_id.is_empty() {
        return Err("Google Client ID is not configured. Set GOOGLE_CLIENT_ID environment variable.".to_string());
    }

    // PKCE コード生成
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    // ローカルリダイレクトサーバーを起動
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind local server: {}", e))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?
        .port();

    let redirect_uri = format!("http://127.0.0.1:{}", port);

    // 認証URLを構築（plain method を使用）
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=plain&access_type=offline&prompt=consent",
        AUTH_URL,
        urlencoding_encode(&client_id),
        urlencoding_encode(&redirect_uri),
        urlencoding_encode(SCOPES),
        urlencoding_encode(&code_challenge),
    );

    // ブラウザで認証URLを開く
    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        return Err(format!("Failed to open auth URL: {}", e));
    }

    // コールバックを待つ（タイムアウト: 5分）
    let auth_code = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        wait_for_auth_code(listener),
    )
    .await
    .map_err(|_| "OAuth timeout: authentication took too long".to_string())?
    .map_err(|e| format!("Failed to receive auth code: {}", e))?;

    // 認証コードをトークンに交換
    let token_response = exchange_code_for_token(
        &client_id,
        &auth_code,
        &redirect_uri,
        &code_verifier,
    )
    .await?;

    // トークンを保存
    token_save(app, ACCESS_TOKEN_KEY, &token_response.access_token)?;

    if let Some(ref refresh_token) = token_response.refresh_token {
        token_save(app, REFRESH_TOKEN_KEY, refresh_token)?;
    }

    // 有効期限を保存
    let expiry = chrono::Local::now()
        + chrono::Duration::seconds(token_response.expires_in as i64);
    token_save(app, TOKEN_EXPIRY_KEY, &expiry.to_rfc3339())?;

    info!("OAuth2 authentication successful");
    Ok("Authentication successful".to_string())
}

/// 認証コードを待つ（ローカルHTTPサーバー）
async fn wait_for_auth_code(
    listener: tokio::net::TcpListener,
) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| format!("Failed to accept connection: {}", e))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read: {}", e))?;

    let request = String::from_utf8_lossy(&buf[..n]);

    // リクエストから code パラメータを抽出
    let code = extract_query_param(&request, "code")
        .ok_or("Authorization code not found in callback".to_string())?;

    // 成功レスポンスを返す
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>認証成功！</h1><p>このタブを閉じてMeetingRecに戻ってください。</p><script>window.close();</script></body></html>";
    let _ = stream.write_all(response.as_bytes()).await;

    Ok(code)
}

/// HTTPリクエストからクエリパラメータを抽出
fn extract_query_param(request: &str, param: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            if key == param {
                return Some(urlencoding_decode(value));
            }
        }
    }
    None
}

/// 認証コードをトークンに交換
async fn exchange_code_for_token(
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, String> {
    let client = Client::new();

    let params = [
        ("client_id", client_id),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", error_body));
    }

    response
        .json::<TokenResponse>()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))
}

/// アクセストークンを取得（必要ならリフレッシュ）
pub async fn get_valid_access_token(app: &AppHandle) -> Result<String, String> {
    // 有効期限チェック
    if let Some(expiry_str) = token_load(app, TOKEN_EXPIRY_KEY)? {
        if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(&expiry_str) {
            let now = chrono::Local::now();
            if now < expiry.with_timezone(&chrono::Local) - chrono::Duration::minutes(5) {
                // まだ有効
                if let Some(token) = token_load(app, ACCESS_TOKEN_KEY)? {
                    return Ok(token);
                }
            }
        }
    }

    // リフレッシュが必要
    refresh_access_token(app).await
}

/// リフレッシュトークンで新しいアクセストークンを取得
async fn refresh_access_token(app: &AppHandle) -> Result<String, String> {
    let client_id = get_client_id();
    let refresh_token = token_load(app, REFRESH_TOKEN_KEY)?
        .ok_or("No refresh token found. Please re-authenticate.".to_string())?;

    let client = Client::new();

    let params = [
        ("client_id", client_id.as_str()),
        ("refresh_token", refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {}", e))?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed: {}", error_body));
    }

    let token_resp: TokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {}", e))?;

    // 新しいトークンを保存
    token_save(app, ACCESS_TOKEN_KEY, &token_resp.access_token)?;

    let expiry = chrono::Local::now()
        + chrono::Duration::seconds(token_resp.expires_in as i64);
    token_save(app, TOKEN_EXPIRY_KEY, &expiry.to_rfc3339())?;

    Ok(token_resp.access_token)
}

/// 認証ステータスを返す
pub async fn check_auth_status(app: &AppHandle) -> AuthStatus {
    match get_valid_access_token(app).await {
        Ok(token) => {
            // ユーザーメール取得
            let client = Client::new();
            let email = client
                .get(USERINFO_URL)
                .bearer_auth(&token)
                .send()
                .await
                .ok()
                .and_then(|r| {
                    if r.status().is_success() {
                        Some(r)
                    } else {
                        None
                    }
                });

            let user_email = if let Some(resp) = email {
                resp.json::<UserInfoResponse>()
                    .await
                    .ok()
                    .map(|u| u.email)
            } else {
                None
            };

            AuthStatus {
                is_authenticated: true,
                user_email,
            }
        }
        Err(_) => AuthStatus {
            is_authenticated: false,
            user_email: None,
        },
    }
}

/// Google Drive にファイルをアップロード
pub async fn upload_file(
    app: &AppHandle,
    file_path: &str,
    file_name: &str,
    folder_name: &str,
) -> Result<(), String> {
    let token = get_valid_access_token(app).await?;
    let client = Client::new();

    // 1. フォルダを検索 or 作成
    let folder_id = find_or_create_folder(&client, &token, folder_name).await?;

    // 2. ファイルを読み込み
    let file_bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // 3. アップロード進捗をフロントエンドに通知
    let _ = app.emit(
        "upload-progress",
        UploadProgressPayload {
            file_name: file_name.to_string(),
            progress_percent: 0.0,
            status: "uploading".to_string(),
        },
    );

    // 4. メタデータ
    let metadata = serde_json::json!({
        "name": file_name,
        "parents": [folder_id]
    });

    // 5. Multipart upload
    let metadata_part = reqwest::multipart::Part::text(metadata.to_string())
        .mime_str("application/json")
        .map_err(|e| format!("Failed to create metadata part: {}", e))?;

    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name.to_string())
        .mime_str("video/mp4")
        .map_err(|e| format!("Failed to create file part: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .part("metadata", metadata_part)
        .part("file", file_part);

    let response = client
        .post(format!("{}?uploadType=multipart", DRIVE_UPLOAD_URL))
        .bearer_auth(&token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload request failed: {}", e))?;

    if response.status().is_success() {
        info!("File '{}' uploaded successfully to Google Drive", file_name);

        let _ = app.emit(
            "upload-progress",
            UploadProgressPayload {
                file_name: file_name.to_string(),
                progress_percent: 100.0,
                status: "completed".to_string(),
            },
        );

        Ok(())
    } else {
        let error_body = response.text().await.unwrap_or_default();
        let _ = app.emit(
            "upload-progress",
            UploadProgressPayload {
                file_name: file_name.to_string(),
                progress_percent: 0.0,
                status: "error".to_string(),
            },
        );
        Err(format!("Upload failed: {}", error_body))
    }
}

/// Google Drive でフォルダを検索 or 作成
async fn find_or_create_folder(
    client: &Client,
    token: &str,
    folder_name: &str,
) -> Result<String, String> {
    let query = format!(
        "name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false",
        folder_name
    );

    let response = client
        .get(DRIVE_FILES_URL)
        .bearer_auth(token)
        .query(&[("q", query.as_str()), ("fields", "files(id,name)")])
        .send()
        .await
        .map_err(|e| format!("Folder search failed: {}", e))?;

    #[derive(Deserialize)]
    struct FileList {
        files: Vec<FileInfo>,
    }
    #[derive(Deserialize)]
    struct FileInfo {
        id: String,
    }

    if response.status().is_success() {
        let file_list: FileList = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse folder search: {}", e))?;

        if let Some(folder) = file_list.files.first() {
            return Ok(folder.id.clone());
        }
    }

    // フォルダ作成
    let metadata = serde_json::json!({
        "name": folder_name,
        "mimeType": "application/vnd.google-apps.folder"
    });

    let response = client
        .post(DRIVE_FILES_URL)
        .bearer_auth(token)
        .json(&metadata)
        .send()
        .await
        .map_err(|e| format!("Folder creation failed: {}", e))?;

    if response.status().is_success() {
        let folder: FileInfo = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse folder creation response: {}", e))?;
        Ok(folder.id)
    } else {
        let error_body = response.text().await.unwrap_or_default();
        Err(format!("Failed to create folder: {}", error_body))
    }
}

// --- ユーティリティ関数 ---

/// PKCE code_verifier 生成
fn generate_code_verifier() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut result = String::with_capacity(64);
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

    for _ in 0..64 {
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        hasher.write_u64(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64);
        let idx = (hasher.finish() as usize) % chars.len();
        result.push(chars.as_bytes()[idx] as char);
    }

    result
}

/// PKCE code_challenge 生成 (plain method)
fn generate_code_challenge(verifier: &str) -> String {
    verifier.to_string()
}

/// 簡易 URL エンコード
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            ':' => result.push_str("%3A"),
            '/' => result.push_str("%2F"),
            '?' => result.push_str("%3F"),
            '&' => result.push_str("%26"),
            '=' => result.push_str("%3D"),
            '@' => result.push_str("%40"),
            '+' => result.push_str("%2B"),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// 簡易 URL デコード
fn urlencoding_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &String::from_utf8_lossy(&bytes[i + 1..i + 3]),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::drive;
use crate::models::*;
use crate::scheduler::{self, SchedulerState};

// =====================================================
// スケジュール関連コマンド
// =====================================================

/// スケジュール一覧を取得
#[tauri::command]
pub async fn list_schedules(app: AppHandle) -> Result<Vec<Schedule>, String> {
    Ok(scheduler::load_schedules(&app))
}

/// スケジュールを追加
#[tauri::command]
pub async fn add_schedule(app: AppHandle, schedule: Schedule) -> Result<Schedule, String> {
    let mut schedules = scheduler::load_schedules(&app);

    // IDが空の場合は新規生成
    let mut new_schedule = schedule;
    if new_schedule.id.is_empty() {
        new_schedule.id = uuid::Uuid::new_v4().to_string();
    }

    schedules.push(new_schedule.clone());
    scheduler::save_schedules(&app, &schedules);

    // スケジューラーに再計算を通知
    let state = app.state::<Arc<SchedulerState>>();
    state.notify.notify_one();

    Ok(new_schedule)
}

/// スケジュールを更新
#[tauri::command]
pub async fn update_schedule(app: AppHandle, schedule: Schedule) -> Result<(), String> {
    let mut schedules = scheduler::load_schedules(&app);

    if let Some(existing) = schedules.iter_mut().find(|s| s.id == schedule.id) {
        *existing = schedule;
    } else {
        return Err("Schedule not found".to_string());
    }

    scheduler::save_schedules(&app, &schedules);

    // スケジューラーに再計算を通知
    let state = app.state::<Arc<SchedulerState>>();
    state.notify.notify_one();

    Ok(())
}

/// スケジュールを削除
#[tauri::command]
pub async fn delete_schedule(app: AppHandle, id: String) -> Result<(), String> {
    let mut schedules = scheduler::load_schedules(&app);
    let initial_len = schedules.len();
    schedules.retain(|s| s.id != id);

    if schedules.len() == initial_len {
        return Err("Schedule not found".to_string());
    }

    scheduler::save_schedules(&app, &schedules);

    // スケジューラーに再計算を通知
    let state = app.state::<Arc<SchedulerState>>();
    state.notify.notify_one();

    Ok(())
}

/// スケジュールの有効/無効を切り替え
#[tauri::command]
pub async fn toggle_schedule(
    app: AppHandle,
    id: String,
    active: bool,
) -> Result<(), String> {
    let mut schedules = scheduler::load_schedules(&app);

    if let Some(schedule) = schedules.iter_mut().find(|s| s.id == id) {
        schedule.active = active;
    } else {
        return Err("Schedule not found".to_string());
    }

    scheduler::save_schedules(&app, &schedules);

    // スケジューラーに再計算を通知
    let state = app.state::<Arc<SchedulerState>>();
    state.notify.notify_one();

    Ok(())
}

// =====================================================
// Google Drive 関連コマンド
// =====================================================

/// Google OAuth2 認証フローを開始
#[tauri::command]
pub async fn start_google_auth(app: AppHandle) -> Result<String, String> {
    drive::start_oauth(&app).await
}

/// 認証ステータスを取得
#[tauri::command]
pub async fn get_auth_status(app: AppHandle) -> Result<AuthStatus, String> {
    Ok(drive::check_auth_status(&app).await)
}

/// ファイルを Google Drive にアップロード
#[tauri::command]
pub async fn upload_to_drive(
    app: AppHandle,
    file_path: String,
    file_name: String,
) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    let folder_name = store
        .get("drive_folder_name")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Meeting Records".to_string());

    drive::upload_file(&app, &file_path, &file_name, &folder_name).await
}

/// Drive 設定を取得
#[tauri::command]
pub async fn get_drive_config(app: AppHandle) -> Result<DriveConfig, String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    let folder_name = store
        .get("drive_folder_name")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Meeting Records".to_string());

    let delete_after_upload = store
        .get("drive_delete_after_upload")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(DriveConfig {
        folder_name,
        delete_after_upload,
    })
}

/// Drive 設定を保存
#[tauri::command]
pub async fn set_drive_config(app: AppHandle, config: DriveConfig) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    store.set(
        "drive_folder_name",
        serde_json::Value::String(config.folder_name),
    );
    store.set(
        "drive_delete_after_upload",
        serde_json::Value::Bool(config.delete_after_upload),
    );
    store
        .save()
        .map_err(|e| format!("Failed to save settings: {}", e))?;

    Ok(())
}

// =====================================================
// 録画設定関連コマンド
// =====================================================

/// 録画設定を取得
#[tauri::command]
pub async fn get_recording_config(app: AppHandle) -> Result<RecordingConfig, String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    match store.get("recording_config") {
        Some(val) => serde_json::from_value(val.clone())
            .map_err(|e| format!("Failed to parse recording config: {}", e)),
        None => Ok(RecordingConfig::default()),
    }
}

/// 録画設定を保存
#[tauri::command]
pub async fn save_recording_config(
    app: AppHandle,
    config: RecordingConfig,
) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    let val =
        serde_json::to_value(&config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    store.set("recording_config", val);
    store
        .save()
        .map_err(|e| format!("Failed to save settings: {}", e))?;

    Ok(())
}

/// 録画の保存先ディレクトリパスを取得
#[tauri::command]
pub async fn get_recordings_dir() -> Result<String, String> {
    let video_dir = dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .ok_or("Could not determine video directory".to_string())?;

    let recordings_dir = video_dir.join("MeetingRec");

    // ディレクトリが無ければ作成
    if !recordings_dir.exists() {
        std::fs::create_dir_all(&recordings_dir)
            .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
    }

    Ok(recordings_dir
        .to_string_lossy()
        .to_string())
}

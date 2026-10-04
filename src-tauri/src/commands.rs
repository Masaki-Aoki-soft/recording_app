use tauri::{AppHandle, Manager};

use crate::drive;
use crate::models::*;
use crate::recorder::audio;
use crate::scheduler;
use crate::session::{self, SessionKind, SessionManager};
use crate::settings;
use crate::zoom;
use crate::AppFlags;

// =====================================================
// スケジュール関連コマンド
// =====================================================

fn validate_schedule(schedule: &Schedule) -> Result<(), String> {
    if schedule.name.trim().is_empty() {
        return Err("会議名を入力してください".into());
    }
    if zoom::parse_meeting_url(&schedule.url).is_none() {
        return Err("Zoom の会議 URL（https://zoom.us/j/... など）を入力してください".into());
    }
    match &schedule.schedule_type {
        ScheduleType::Once { datetime } => {
            chrono::DateTime::parse_from_rfc3339(datetime)
                .map_err(|_| "日時の形式が正しくありません".to_string())?;
        }
        ScheduleType::Weekly {
            day_of_week,
            hour,
            minute,
        } => {
            if *day_of_week > 6 || *hour > 23 || *minute > 59 {
                return Err("曜日・時刻の指定が正しくありません".into());
            }
        }
    }
    if schedule.duration_minutes == Some(0) {
        return Err("録画時間は 1 分以上を指定してください".into());
    }
    Ok(())
}

/// スケジュール一覧を取得（次回実行日時付き）
#[tauri::command]
pub async fn list_schedules(app: AppHandle) -> Result<Vec<Schedule>, String> {
    let now = chrono::Local::now();
    Ok(scheduler::load_schedules(&app)
        .into_iter()
        .map(|mut s| {
            s.next_run = s
                .active
                .then(|| scheduler::next_run(&s, now))
                .flatten()
                .map(|t| t.to_rfc3339());
            s
        })
        .collect())
}

/// スケジュールを追加
#[tauri::command]
pub async fn add_schedule(app: AppHandle, schedule: Schedule) -> Result<Schedule, String> {
    validate_schedule(&schedule)?;
    let mut schedules = scheduler::load_schedules(&app);

    let mut new_schedule = schedule;
    new_schedule.next_run = None;
    if new_schedule.id.is_empty() {
        new_schedule.id = uuid::Uuid::new_v4().to_string();
    }

    schedules.push(new_schedule.clone());
    scheduler::save_schedules(&app, &schedules)?;
    Ok(new_schedule)
}

/// スケジュールを更新
#[tauri::command]
pub async fn update_schedule(app: AppHandle, schedule: Schedule) -> Result<(), String> {
    validate_schedule(&schedule)?;
    let mut schedules = scheduler::load_schedules(&app);

    let existing = schedules
        .iter_mut()
        .find(|s| s.id == schedule.id)
        .ok_or("スケジュールが見つかりません")?;
    *existing = Schedule {
        next_run: None,
        ..schedule
    };

    scheduler::save_schedules(&app, &schedules)
}

/// スケジュールを削除
#[tauri::command]
pub async fn delete_schedule(app: AppHandle, id: String) -> Result<(), String> {
    let mut schedules = scheduler::load_schedules(&app);
    let initial_len = schedules.len();
    schedules.retain(|s| s.id != id);

    if schedules.len() == initial_len {
        return Err("スケジュールが見つかりません".into());
    }
    scheduler::save_schedules(&app, &schedules)
}

/// スケジュールの有効/無効を切り替え
#[tauri::command]
pub async fn toggle_schedule(app: AppHandle, id: String, active: bool) -> Result<(), String> {
    let mut schedules = scheduler::load_schedules(&app);
    schedules
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or("スケジュールが見つかりません")?
        .active = active;
    scheduler::save_schedules(&app, &schedules)
}

/// スケジュールを今すぐ実行（Zoom 参加 → 録画）
#[tauri::command]
pub async fn run_schedule_now(app: AppHandle, id: String) -> Result<(), String> {
    let schedule = scheduler::load_schedules(&app)
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("スケジュールが見つかりません")?;
    session::start(&app, SessionKind::Scheduled(schedule))
}

// =====================================================
// 録画セッション
// =====================================================

#[tauri::command]
pub async fn get_session_status(app: AppHandle) -> Result<SessionStatus, String> {
    Ok(app.state::<SessionManager>().status())
}

#[tauri::command]
pub async fn start_manual_recording(app: AppHandle) -> Result<(), String> {
    session::start(&app, SessionKind::Manual)
}

#[tauri::command]
pub async fn stop_session(app: AppHandle) -> Result<(), String> {
    if app.state::<SessionManager>().request_stop() {
        Ok(())
    } else {
        Err("実行中の録画はありません".into())
    }
}

/// ローカルの録画ファイル一覧（新しい順）
#[tauri::command]
pub async fn list_recordings(app: AppHandle) -> Result<Vec<RecordingEntry>, String> {
    let dir = settings::recordings_dir()?;
    let uploaded = drive::uploaded_files(&app);

    let mut entries: Vec<RecordingEntry> = std::fs::read_dir(&dir)
        .map_err(|e| format!("保存先フォルダを読めません: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let modified: chrono::DateTime<chrono::Local> = meta.modified().ok()?.into();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            Some(RecordingEntry {
                path: entry.path().to_string_lossy().into_owned(),
                uploaded: uploaded.contains(&file_name),
                file_name,
                size_bytes: meta.len(),
                modified_at: modified.to_rfc3339(),
            })
        })
        .collect();

    entries.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(entries)
}

#[tauri::command]
pub async fn open_recordings_dir() -> Result<(), String> {
    let dir = settings::recordings_dir()?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

// =====================================================
// 設定
// =====================================================

#[tauri::command]
pub async fn get_recording_config(app: AppHandle) -> Result<RecordingConfig, String> {
    Ok(settings::load(&app, settings::KEY_RECORDING_CONFIG))
}

#[tauri::command]
pub async fn save_recording_config(app: AppHandle, config: RecordingConfig) -> Result<(), String> {
    settings::save(&app, settings::KEY_RECORDING_CONFIG, &config)
}

#[tauri::command]
pub async fn get_general_settings(app: AppHandle) -> Result<GeneralSettings, String> {
    Ok(settings::load(&app, settings::KEY_GENERAL_SETTINGS))
}

#[tauri::command]
pub async fn save_general_settings(app: AppHandle, settings: GeneralSettings) -> Result<(), String> {
    crate::settings::save(&app, crate::settings::KEY_GENERAL_SETTINGS, &settings)?;
    app.state::<std::sync::Arc<scheduler::SchedulerState>>()
        .notify
        .notify_one();
    Ok(())
}

/// マイク（入力デバイス）の一覧
#[tauri::command]
pub async fn get_mic_devices() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(audio::list_input_devices)
        .await
        .map_err(|e| e.to_string())?
}

/// Clerk のサインイン状態（サインイン中のみスケジュール録画を行う）
#[tauri::command]
pub async fn set_signed_in(app: AppHandle, signed_in: bool) -> Result<(), String> {
    app.state::<AppFlags>().set_signed_in(&app, signed_in)
}

// =====================================================
// Google Drive
// =====================================================

#[tauri::command]
pub async fn start_google_auth(app: AppHandle) -> Result<(), String> {
    drive::start_oauth(&app).await
}

#[tauri::command]
pub async fn disconnect_google(app: AppHandle) -> Result<(), String> {
    drive::disconnect(&app).await
}

#[tauri::command]
pub async fn get_drive_auth_status(app: AppHandle) -> Result<DriveAuthStatus, String> {
    Ok(drive::auth_status(&app))
}

#[tauri::command]
pub async fn get_drive_config(app: AppHandle) -> Result<DriveConfig, String> {
    Ok(settings::load(&app, settings::KEY_DRIVE_CONFIG))
}

#[tauri::command]
pub async fn set_drive_config(app: AppHandle, config: DriveConfig) -> Result<(), String> {
    settings::save(&app, settings::KEY_DRIVE_CONFIG, &config)
}

/// 録画ファイルを Drive へアップロード
#[tauri::command]
pub async fn upload_recording(app: AppHandle, path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    let dir = settings::recordings_dir()?;
    // 保存先フォルダ外の任意ファイルはアップロードさせない
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(dir.canonicalize().map_err(|e| e.to_string())?) {
        return Err("録画フォルダ内のファイルのみアップロードできます".into());
    }
    drive::enqueue(&app, &path)?;
    drive::process_queue(&app).await
}

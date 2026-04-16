use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::drive;
use crate::models::*;
use crate::scheduler::{self, SchedulerState};

// =====================================================
// FFmpeg サイドカーバイナリのパス解決
// =====================================================

/// src-tauri/bin に配置された FFmpeg サイドカーバイナリのパスを解決する。
/// Tauri のビルドシステムが externalBin で指定されたバイナリを
/// 実行ファイルと同じディレクトリにコピーするため、current_exe() の
/// 親ディレクトリから検索する。
fn resolve_ffmpeg_path() -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe path: {}", e))?
        .parent()
        .ok_or_else(|| "Failed to get exe directory".to_string())?
        .to_path_buf();

    let binary_name = if cfg!(windows) {
        format!("ffmpeg-{}.exe", env!("TAURI_ENV_TARGET_TRIPLE"))
    } else {
        format!("ffmpeg-{}", env!("TAURI_ENV_TARGET_TRIPLE"))
    };

    // 1. exeと同じディレクトリ（本番環境＆コピー済み開発環境）
    let path1 = exe_dir.join(&binary_name);
    if path1.exists() {
        return Ok(path1);
    }

    // 2. 開発環境のフォールバック (src-tauri/bin)
    // target/debug から target, src-tauri へと遡る
    if let (Some(target_dir), Some(src_tauri_dir)) = (exe_dir.parent(), exe_dir.parent().and_then(|p| p.parent())) {
        let path2 = src_tauri_dir.join("bin").join(&binary_name);
        if path2.exists() {
            return Ok(path2);
        }
    }

    // 3. コマンド実行ディレクトリからの相対パス
    let path3 = std::env::current_dir()
        .unwrap_or_default()
        .join("bin")
        .join(&binary_name);
    if path3.exists() {
        return Ok(path3);
    }

    Err(format!(
        "FFmpeg sidecar not found. Make sure {} is placed in src-tauri/bin/. Searched paths: {}, {}",
        binary_name,
        path1.display(),
        exe_dir.join("../../bin").join(&binary_name).display()
    ))
}

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

/// 録音デバイスの一覧を取得
#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<String>, String> {
    crate::audio_capture::get_host_mic_devices()
}

// =====================================================
// FFmpeg 録画制御 (Rust)
// =====================================================

#[tauri::command]
pub async fn start_recording(
    config: RecordingConfig,
    state: tauri::State<'_, crate::AppState>,
) -> Result<String, String> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    {
        let process_lock = state.ffmpeg_process.lock().unwrap();
        if process_lock.is_some() {
            return Err("Already recording".to_string());
        }
    }

    let video_dir = get_recordings_dir().await?;
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let filepath = format!("{}\\MeetingRec_{}.mp4", video_dir, timestamp);

    let mut args = vec!["-y".to_string()];
    let resolution = match config.resolution.as_str() {
        "720p" => "1280x720",
        "4k" => "3840x2160",
        _ => "1920x1080",
    };

    let mut input_count = 1; // video (desktop) is input 0
    let mut audio_inputs = 0;

    args.push("-f".to_string());
    args.push("gdigrab".to_string());
    args.push("-framerate".to_string());
    args.push(config.framerate.to_string());
    args.push("-video_size".to_string());
    args.push(resolution.to_string());
    args.push("-i".to_string());
    args.push("desktop".to_string());

    state.audio_is_running.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut local_streams = Vec::new();

    if config.capture_system_audio {
        if let Some(dev) = &config.audio_device {
            if !dev.is_empty() {
                let pipe_name = format!("tauri_sys_{}", timestamp);
                match crate::audio_capture::start_capture_stream(
                    dev.clone(),
                    false, // is_input = false means loopback of an output
                    pipe_name.clone(),
                    state.audio_is_running.clone(),
                ).await {
                    Ok((stream, _, sample_rate, channels)) => {
                        local_streams.push(stream);
                        args.push("-f".to_string());
                        args.push("f32le".to_string());
                        args.push("-ar".to_string());
                        args.push(sample_rate.to_string());
                        args.push("-ac".to_string());
                        args.push(channels.to_string());
                        args.push("-i".to_string());
                        args.push(format!(r"\\.\pipe\{}", pipe_name));
                        input_count += 1;
                        audio_inputs += 1;
                    }
                    Err(e) => {
                        log::error!("Failed to start system audio stream: {}", e);
                    }
                }
            }
        }
    }

    if config.capture_mic {
        if let Some(dev) = &config.mic_device {
            if !dev.is_empty() {
                let pipe_name = format!("tauri_mic_{}", timestamp);
                match crate::audio_capture::start_capture_stream(
                    dev.clone(),
                    true, // is_input = true
                    pipe_name.clone(),
                    state.audio_is_running.clone(),
                ).await {
                    Ok((stream, _, sample_rate, channels)) => {
                        local_streams.push(stream);
                        args.push("-f".to_string());
                        args.push("f32le".to_string());
                        args.push("-ar".to_string());
                        args.push(sample_rate.to_string());
                        args.push("-ac".to_string());
                        args.push(channels.to_string());
                        args.push("-i".to_string());
                        args.push(format!(r"\\.\pipe\{}", pipe_name));
                        input_count += 1;
                        audio_inputs += 1;
                    }
                    Err(e) => {
                        log::error!("Failed to start mic audio stream: {}", e);
                    }
                }
            }
        }
    }

    if audio_inputs > 1 {
        // 複数オーディオの場合はミックスする
        args.push("-filter_complex".to_string());
        args.push(format!("amix=inputs={}:duration=first:dropout_transition=2", audio_inputs));
    }

    {
        let mut audio_streams = state.audio_streams.lock().unwrap();
        audio_streams.clear();
        audio_streams.append(&mut local_streams);
    }

    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.push("-vcodec".to_string());
    args.push("libx264".to_string());
    args.push("-preset".to_string());
    args.push("ultrafast".to_string());
    args.push("-tune".to_string());
    args.push("zerolatency".to_string());
    args.push("-acodec".to_string());
    args.push("aac".to_string());
    args.push("-b:a".to_string());
    args.push("128k".to_string());
    args.push(filepath.clone());

    let ffmpeg_path = resolve_ffmpeg_path()?;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut child = Command::new(&ffmpeg_path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped()) // Capture stderr for debugging!
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    log::info!("FFmpeg: {}", line);
                }
            }
        });
    }

    *state.ffmpeg_process.lock().unwrap() = Some(child);
    *state.output_path.lock().unwrap() = Some(filepath.clone());

    Ok(filepath)
}

#[tauri::command]
pub async fn stop_recording(state: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    // 録音スレッドを停止
    state.audio_is_running.store(false, std::sync::atomic::Ordering::SeqCst);
    {
        let mut audio_streams = state.audio_streams.lock().unwrap();
        audio_streams.clear(); // Drop causes cpal to stop capturing
    }

    let mut process_lock = state.ffmpeg_process.lock().unwrap();
    if let Some(mut child) = process_lock.take() {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(b"q");
            let _ = stdin.flush();
        }
        
        // Wait for it to gracefully exit
        let _ = child.wait();
        
        let path = state.output_path.lock().unwrap().take().unwrap_or_default();
        return Ok(path);
    }
    
    Err("Not recording".to_string())
}

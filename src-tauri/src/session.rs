//! 録画セッションのオーケストレーション
//!
//! Zoom 参加 → 会議ウィンドウ待ち → 録画 → 会議終了/指定時間/手動で停止 → mp4 化 → Drive アップロード
//! をすべて Rust 側で実行する（フロントが非表示・未ロードでも動作する）。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

use crate::drive;
use crate::models::{
    DriveConfig, GeneralSettings, RecordingConfig, Schedule, SessionState, SessionStatus,
};
use crate::recorder::{CaptureTarget, Recording};
use crate::settings;
use crate::zoom;

/// 会議ウィンドウが連続でこの秒数見つからなければ会議終了とみなす
const MEETING_END_GRACE_SECS: u64 = 10;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub enum SessionKind {
    /// スケジュールによる自動参加 + 録画
    Scheduled(Schedule),
    /// 手動録画（Zoom 会議ウィンドウがあればそれ、無ければ画面全体）
    Manual,
}

/// アプリ全体で 1 つのセッションマネージャ
pub struct SessionManager {
    status: Mutex<SessionStatus>,
    /// 実行中セッションの停止シグナル
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(SessionStatus::idle()),
            stop_tx: Mutex::new(None),
        }
    }

    pub fn status(&self) -> SessionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn is_busy(&self) -> bool {
        self.stop_tx.lock().unwrap().is_some()
    }

    /// 実行中のセッションに停止を要求する
    pub fn request_stop(&self) -> bool {
        match self.stop_tx.lock().unwrap().as_ref() {
            Some(tx) => {
                let _ = tx.send(true);
                true
            }
            None => false,
        }
    }
}

fn set_status(app: &AppHandle, status: SessionStatus) {
    let manager = app.state::<SessionManager>();
    *manager.status.lock().unwrap() = status.clone();
    let _ = app.emit("session-status", &status);
    crate::update_tray(app, &status);
}

/// セッションを開始する。既に実行中の場合はエラー
pub fn start(app: &AppHandle, kind: SessionKind) -> Result<(), String> {
    let manager = app.state::<SessionManager>();
    let stop_rx = {
        let mut guard = manager.stop_tx.lock().unwrap();
        if guard.is_some() {
            return Err("別の録画セッションが実行中です".into());
        }
        let (tx, rx) = watch::channel(false);
        *guard = Some(tx);
        rx
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let name = match &kind {
            SessionKind::Scheduled(s) => Some(s.name.clone()),
            SessionKind::Manual => None,
        };
        let result = run(&app, kind, stop_rx).await;

        let status = match result {
            Ok(message) => SessionStatus {
                message,
                ..SessionStatus::idle()
            },
            Err(e) => {
                error!("Session failed: {}", e);
                SessionStatus {
                    state: SessionState::Error,
                    schedule_name: name,
                    message: Some(e),
                    ..SessionStatus::idle()
                }
            }
        };
        *app.state::<SessionManager>().stop_tx.lock().unwrap() = None;
        set_status(&app, status);
    });
    Ok(())
}

/// 停止シグナルを待つ
async fn stopped(rx: &mut watch::Receiver<bool>) {
    let _ = rx.wait_for(|stop| *stop).await;
}

async fn run(
    app: &AppHandle,
    kind: SessionKind,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<Option<String>, String> {
    let general: GeneralSettings = settings::load(app, settings::KEY_GENERAL_SETTINGS);
    let rec_config: RecordingConfig = settings::load(app, settings::KEY_RECORDING_CONFIG);

    let (name, duration_minutes, target) = match kind {
        SessionKind::Scheduled(schedule) => {
            let status = |state| SessionStatus {
                state,
                schedule_name: Some(schedule.name.clone()),
                ..SessionStatus::idle()
            };

            // 1. Zoom アプリで参加
            set_status(app, status(SessionState::Launching));
            let meeting = zoom::parse_meeting_url(&schedule.url)
                .ok_or_else(|| format!("Zoom の会議 URL ではありません: {}", schedule.url))?;
            zoom::join(&meeting, &general.zoom_display_name)?;

            // 2. 会議ウィンドウが出るまで待つ（待機室・ホスト未開始を考慮）
            set_status(app, status(SessionState::WaitingForMeeting));
            let timeout = Duration::from_secs(general.wait_timeout_minutes.max(1) as u64 * 60);
            let hwnd = tokio::select! {
                hwnd = wait_for_meeting_window(timeout) => hwnd,
                _ = stopped(&mut stop_rx) => return Ok(Some("会議待ちをキャンセルしました".into())),
            };
            let hwnd = hwnd.ok_or_else(|| {
                format!(
                    "{} 分待ちましたが Zoom の会議ウィンドウが見つかりませんでした",
                    general.wait_timeout_minutes
                )
            })?;
            (
                Some(schedule.name),
                schedule.duration_minutes,
                CaptureTarget::Window(hwnd),
            )
        }
        SessionKind::Manual => {
            let target = zoom::find_meeting_window()
                .map(CaptureTarget::Window)
                .unwrap_or(CaptureTarget::PrimaryMonitor);
            (None, None, target)
        }
    };

    // 3. 録画開始
    let output_path = output_path_for(name.as_deref())?;
    let rec_path = output_path.clone();
    let (mut recording, warnings) = tauri::async_runtime::spawn_blocking(move || {
        Recording::start(target, &rec_config, &rec_path)
    })
    .await
    .map_err(|e| e.to_string())??;

    let _power = PowerGuard::new();
    let started_at = chrono::Local::now();
    set_status(
        app,
        SessionStatus {
            state: SessionState::Recording,
            schedule_name: name.clone(),
            started_at: Some(started_at.to_rfc3339()),
            output_path: Some(output_path.to_string_lossy().into_owned()),
            message: (!warnings.is_empty()).then(|| warnings.join(" / ")),
        },
    );
    info!("Recording started: {}", output_path.display());

    // 4. 停止条件を監視
    let deadline = duration_minutes.map(|m| Instant::now() + Duration::from_secs(m as u64 * 60));
    let watch_meeting = matches!(target, CaptureTarget::Window(_));
    let mut missing_since: Option<Instant> = None;
    let mut failure: Option<String> = None;

    let stop_reason = loop {
        tokio::select! {
            _ = stopped(&mut stop_rx) => break "手動で停止しました",
            _ = sleep(POLL_INTERVAL) => {}
        }

        if deadline.is_some_and(|d| Instant::now() >= d) {
            break "指定の録画時間に達しました";
        }
        if !recording.is_alive() {
            failure = Some("録画プロセスが予期せず終了した".into());
            break "エラー";
        }
        if watch_meeting {
            if zoom::find_meeting_window().is_some() && !recording.is_target_closed() {
                missing_since = None;
            } else {
                let since = *missing_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(MEETING_END_GRACE_SECS) {
                    break "会議が終了しました";
                }
            }
        }
    };
    info!("Stopping recording: {}", stop_reason);

    // 5. mp4 に変換
    set_status(
        app,
        SessionStatus {
            state: SessionState::Finalizing,
            schedule_name: name.clone(),
            started_at: Some(started_at.to_rfc3339()),
            output_path: Some(output_path.to_string_lossy().into_owned()),
            message: Some(stop_reason.to_string()),
        },
    );
    let saved = tauri::async_runtime::spawn_blocking(move || recording.stop_and_finalize())
        .await
        .map_err(|e| e.to_string())??;
    drop(_power);

    // 6. Google Drive へアップロード
    let drive_config: DriveConfig = settings::load(app, settings::KEY_DRIVE_CONFIG);
    let mut file_name = saved
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(e) = failure {
        // 途中までの録画は保存できているので、理由を添えて通知する
        warn!("{}", e);
        file_name = format!("{}（{}ため途中で停止）", file_name, e);
    }

    if drive_config.auto_upload && drive::is_connected(app) {
        set_status(
            app,
            SessionStatus {
                state: SessionState::Uploading,
                schedule_name: name.clone(),
                output_path: Some(saved.to_string_lossy().into_owned()),
                ..SessionStatus::idle()
            },
        );
        drive::enqueue(app, &saved)?;
        if let Err(e) = drive::process_queue(app).await {
            return Ok(Some(format!(
                "{} を保存しました（Drive へのアップロードは後で再試行します: {}）",
                file_name, e
            )));
        }
        return Ok(Some(format!(
            "{} を保存し、Google Drive にアップロードしました",
            file_name
        )));
    }

    Ok(Some(format!("{} を保存しました", file_name)))
}

async fn wait_for_meeting_window(timeout: Duration) -> Option<isize> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(hwnd) = zoom::find_meeting_window() {
            // ウィンドウ生成直後は描画が安定しないので少し待つ
            sleep(Duration::from_secs(2)).await;
            return zoom::find_meeting_window().or(Some(hwnd));
        }
        if Instant::now() >= deadline {
            return None;
        }
        sleep(Duration::from_secs(1)).await;
    }
}

/// 保存先ファイル名: MeetingRec_<会議名>_<日時>.mp4
fn output_path_for(name: Option<&str>) -> Result<PathBuf, String> {
    let dir = settings::recordings_dir()?;
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let file_name = match name.map(sanitize_file_name).filter(|n| !n.is_empty()) {
        Some(n) => format!("MeetingRec_{}_{}.mp4", n, timestamp),
        None => format!("MeetingRec_{}.mp4", timestamp),
    };
    Ok(dir.join(file_name))
}

fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .chars()
        .take(60)
        .collect()
}

/// 録画中に PC がスリープ/画面オフにならないようにする。
/// SetThreadExecutionState はスレッド単位なので専用スレッドで保持する。
struct PowerGuard {
    release: Option<std::sync::mpsc::Sender<()>>,
}

impl PowerGuard {
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                use windows::Win32::System::Power::{
                    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED,
                    ES_SYSTEM_REQUIRED,
                };
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
                let _ = rx.recv();
                SetThreadExecutionState(ES_CONTINUOUS);
            }
            #[cfg(not(windows))]
            let _ = rx.recv();
        });
        Self { release: Some(tx) }
    }
}

impl Drop for PowerGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.release.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_file_name;

    #[test]
    fn sanitizes_windows_reserved_characters() {
        assert_eq!(sanitize_file_name("定例: A/B  "), "定例_ A_B");
        assert_eq!(sanitize_file_name("name."), "name");
    }
}

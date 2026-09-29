use serde::{Deserialize, Serialize};

/// スケジュールの種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ScheduleType {
    /// 単発（特定日時）
    Once {
        datetime: String, // ISO 8601 形式 "2026-04-20T15:00:00+09:00"
    },
    /// 毎週繰り返し
    Weekly {
        day_of_week: u32, // 0=日, 1=月, ..., 6=土
        hour: u32,
        minute: u32,
    },
}

/// スケジュール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    /// Zoom 会議 URL
    pub url: String,
    pub schedule_type: ScheduleType,
    pub active: bool,
    /// 録画時間（分）。None の場合は会議終了（または手動停止）まで録画
    pub duration_minutes: Option<u32>,
    /// 次回の開始予定日時（一覧取得時に計算。保存はしない）
    #[serde(default, skip_deserializing)]
    pub next_run: Option<String>,
}

/// 録画設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingConfig {
    pub resolution: String, // "720p", "1080p", "4k"
    pub framerate: u32,     // 15, 30, 60
    /// システム音声（WASAPI ループバック = 相手の声）
    pub capture_system_audio: bool,
    /// マイク（自分の声）
    pub capture_mic: bool,
    /// マイクのデバイス名。None の場合は既定の入力デバイス
    pub mic_device: Option<String>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            resolution: "1080p".to_string(),
            framerate: 30,
            capture_system_audio: true,
            capture_mic: true,
            mic_device: None,
        }
    }
}

impl RecordingConfig {
    /// 出力の最大解像度（これより大きいキャプチャは縮小する）
    pub fn max_size(&self) -> (u32, u32) {
        match self.resolution.as_str() {
            "720p" => (1280, 720),
            "4k" => (3840, 2160),
            _ => (1920, 1080),
        }
    }

    pub fn fps(&self) -> u32 {
        self.framerate.clamp(5, 60)
    }
}

/// アプリ全般の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    /// Zoom 参加時の表示名
    pub zoom_display_name: String,
    /// 開始時刻の何秒前に Zoom を起動するか
    pub lead_seconds: u32,
    /// 会議ウィンドウが現れるまで待つ最大時間（分）
    pub wait_timeout_minutes: u32,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            zoom_display_name: "MeetingRec".to_string(),
            lead_seconds: 60,
            wait_timeout_minutes: 30,
        }
    }
}

/// Google Drive 設定（Client ID はビルド時に埋め込み）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DriveConfig {
    pub folder_name: String,
    pub delete_after_upload: bool,
    /// 録画完了後に自動でアップロードする
    pub auto_upload: bool,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            folder_name: "Meeting Records".to_string(),
            delete_after_upload: false,
            auto_upload: true,
        }
    }
}

/// Google Drive 連携ステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
}

/// 録画セッションの状態
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Launching,
    WaitingForMeeting,
    Recording,
    Finalizing,
    Uploading,
    Error,
}

/// フロントエンドへ通知するセッションステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub state: SessionState,
    pub schedule_name: Option<String>,
    /// 録画開始日時 (RFC 3339)
    pub started_at: Option<String>,
    pub output_path: Option<String>,
    pub message: Option<String>,
}

impl SessionStatus {
    pub fn idle() -> Self {
        Self {
            state: SessionState::Idle,
            schedule_name: None,
            started_at: None,
            output_path: None,
            message: None,
        }
    }
}

/// ローカルの録画ファイル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingEntry {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub uploaded: bool,
}

/// アップロード進捗ペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadProgressPayload {
    pub file_name: String,
    pub progress_percent: f64,
    pub status: String, // "uploading", "completed", "error"
    pub message: Option<String>,
}

use serde::{Deserialize, Serialize};


/// スケジュールの種類
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub url: String,
    pub schedule_type: ScheduleType,
    pub active: bool,
    /// 録画時間（分）。None の場合は手動停止
    pub duration_minutes: Option<u32>,
}

/// 録画設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    pub resolution: String,    // "720p", "1080p", "4k"
    pub framerate: u32,        // 15, 30, 60
    pub capture_system_audio: bool,
    pub capture_mic: bool,
    pub audio_device: Option<String>,
    pub mic_device: Option<String>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            resolution: "1080p".to_string(),
            framerate: 30,
            capture_system_audio: true,
            capture_mic: true,
            audio_device: None,
            mic_device: None,
        }
    }
}

/// Google Drive 設定（Client IDはビルド時に埋め込み）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    pub folder_name: String,
    pub delete_after_upload: bool,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            folder_name: "Meeting Records".to_string(),
            delete_after_upload: false,
        }
    }
}

/// スケジュール発火時にフロントエンドに送るペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleTriggeredPayload {
    pub schedule_id: String,
    pub schedule_name: String,
    pub url: String,
    pub duration_minutes: Option<u32>,
}

/// Google Drive 認証ステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub is_authenticated: bool,
    pub user_email: Option<String>,
}

/// アップロード進捗ペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadProgressPayload {
    pub file_name: String,
    pub progress_percent: f64,
    pub status: String, // "uploading", "completed", "error"
}

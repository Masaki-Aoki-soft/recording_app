//! settings.json（tauri-plugin-store）の型付き読み書きヘルパー

use serde::{de::DeserializeOwned, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const SETTINGS_STORE: &str = "settings.json";

pub const KEY_RECORDING_CONFIG: &str = "recording_config";
pub const KEY_GENERAL_SETTINGS: &str = "general_settings";
pub const KEY_DRIVE_CONFIG: &str = "drive_config";
pub const KEY_SIGNED_IN: &str = "signed_in";

/// 値を読み込む。未保存・パース失敗時は Default を返す
pub fn load<T: DeserializeOwned + Default>(app: &AppHandle, key: &str) -> T {
    app.store(SETTINGS_STORE)
        .ok()
        .and_then(|store| store.get(key))
        .and_then(|val| serde_json::from_value(val).ok())
        .unwrap_or_default()
}

pub fn save<T: Serialize>(app: &AppHandle, key: &str, value: &T) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|e| format!("設定ストアを開けませんでした: {}", e))?;
    let val = serde_json::to_value(value).map_err(|e| format!("設定のシリアライズに失敗: {}", e))?;
    store.set(key, val);
    store
        .save()
        .map_err(|e| format!("設定の保存に失敗しました: {}", e))
}

/// 録画ファイルの保存先（ビデオ/MeetingRec）。無ければ作成する
pub fn recordings_dir() -> Result<std::path::PathBuf, String> {
    let video_dir = dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .ok_or("ビデオフォルダを特定できませんでした".to_string())?;

    let dir = video_dir.join("MeetingRec");
    std::fs::create_dir_all(&dir).map_err(|e| format!("保存先フォルダを作成できませんでした: {}", e))?;
    Ok(dir)
}

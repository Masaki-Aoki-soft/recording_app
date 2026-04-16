mod audio_capture;
mod commands;
mod drive;
mod models;
mod scheduler;

use std::sync::{Arc, Mutex};
use std::process::Child;
use log::info;

pub struct AppState {
    pub ffmpeg_process: Mutex<Option<Child>>,
    pub output_path: Mutex<Option<String>>,
    pub audio_streams: Mutex<Vec<cpal::Stream>>,
    pub audio_is_running: Arc<std::sync::atomic::AtomicBool>,
}
use tauri::{
    Manager,
    WindowEvent,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(Arc::new(scheduler::SchedulerState::new()))
        .manage(AppState {
            ffmpeg_process: Mutex::new(None),
            output_path: Mutex::new(None),
            audio_streams: Mutex::new(Vec::new()),
            audio_is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_schedules,
            commands::add_schedule,
            commands::update_schedule,
            commands::delete_schedule,
            commands::toggle_schedule,
            commands::start_google_auth,
            commands::get_auth_status,
            commands::upload_to_drive,
            commands::get_drive_config,
            commands::set_drive_config,
            commands::get_recording_config,
            commands::save_recording_config,
            commands::get_recordings_dir,
            commands::get_audio_devices,
            commands::start_recording,
            commands::stop_recording,
        ])
        .setup(|app| {
            // --- システムトレイの設定 ---
            let show_item = MenuItemBuilder::with_id("show", "表示")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "終了")
                .build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            let _tray = TrayIconBuilder::new()
                .tooltip("MeetingRec - 会議録画")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // --- スケジューラーをバックグラウンドで起動 ---
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                scheduler::init_scheduler(handle).await;
            });

            info!("MeetingRec app started successfully");
            Ok(())
        })
        .on_window_event(|window, event| {
            // ✕ ボタンクリック時: 閉じずにトレイに最小化
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                info!("Window hidden to system tray");
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

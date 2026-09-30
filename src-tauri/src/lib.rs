mod clerk_proxy;
mod commands;
mod drive;
mod ffmpeg;
mod models;
mod recorder;
mod scheduler;
mod session;
mod settings;
mod sso;
mod zoom;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{info, warn};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, WindowEvent,
};

use models::{SessionState, SessionStatus};
use session::SessionManager;

const TRAY_ID: &str = "main";
/// 自動起動時に付与する引数（ウィンドウを表示せずトレイに常駐）
const MINIMIZED_ARG: &str = "--minimized";

/// アプリ全体のフラグ
pub struct AppFlags {
    /// Clerk でサインイン中か（サインイン中のみスケジュール録画を行う）
    signed_in: AtomicBool,
}

impl AppFlags {
    pub fn is_signed_in(&self) -> bool {
        self.signed_in.load(Ordering::SeqCst)
    }

    pub fn set_signed_in(&self, app: &AppHandle, value: bool) -> Result<(), String> {
        let previous = self.signed_in.swap(value, Ordering::SeqCst);
        if previous != value {
            info!("Signed-in state changed: {}", value);
            settings::save(app, settings::KEY_SIGNED_IN, &value)?;
            app.state::<Arc<scheduler::SchedulerState>>()
                .notify
                .notify_one();
        }
        Ok(())
    }
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// トレイのツールチップにセッション状態を反映
pub fn update_tray(app: &AppHandle, status: &SessionStatus) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let label = match status.state {
        SessionState::Idle => "待機中",
        SessionState::Launching => "Zoom を起動中",
        SessionState::WaitingForMeeting => "会議の開始待ち",
        SessionState::Recording => "● 録画中",
        SessionState::Finalizing => "動画を保存中",
        SessionState::Uploading => "アップロード中",
        SessionState::Error => "エラー",
    };
    let _ = tray.set_tooltip(Some(format!("MeetingRec - {}", label)));
}

/// 録画中なら安全に停止・保存してから終了する
fn quit_gracefully(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let manager = app.state::<SessionManager>();
        if manager.request_stop() {
            info!("Stopping active session before exit...");
            for _ in 0..600 {
                if !manager.is_busy() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 二重起動時は既存のウィンドウを表示する（最初に登録する必要がある）。
        // deep-link 機能により、meetingrec:// で起動された 2 つ目のプロセスの URL は既存のプロセスへ転送される
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .manage(Arc::new(scheduler::SchedulerState::new()))
        .manage(SessionManager::new())
        .manage(drive::UploadLock(tokio::sync::Mutex::new(())))
        .manage(sso::SsoState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_schedules,
            commands::add_schedule,
            commands::update_schedule,
            commands::delete_schedule,
            commands::toggle_schedule,
            commands::run_schedule_now,
            commands::get_session_status,
            commands::start_manual_recording,
            commands::stop_session,
            commands::list_recordings,
            commands::open_recordings_dir,
            commands::get_recording_config,
            commands::save_recording_config,
            commands::get_general_settings,
            commands::save_general_settings,
            commands::get_mic_devices,
            commands::set_signed_in,
            commands::start_google_auth,
            commands::disconnect_google,
            commands::get_drive_auth_status,
            commands::get_drive_config,
            commands::set_drive_config,
            commands::upload_recording,
            sso::sso_redirect_url,
            sso::wait_for_sso_callback,
            sso::cancel_sso,
            clerk_proxy::clerk_fetch,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // 前回のサインイン状態を復元（フロントの Clerk 読み込み後に上書きされる）
            let signed_in: bool = settings::load(&handle, settings::KEY_SIGNED_IN);
            app.manage(AppFlags {
                signed_in: AtomicBool::new(signed_in),
            });

            drive::migrate_legacy_tokens(&handle);

            // --- Google ログイン（Clerk SSO）のコールバック用 deep link ---
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // インストーラ経由でない起動（bun tauri dev など）でも meetingrec:// を
                // 現在の実行ファイルに関連付ける（HKCU に登録）
                if let Err(e) = app.deep_link().register_all() {
                    warn!("Failed to register deep link scheme: {}", e);
                }
                let deep_link_handle = handle.clone();
                app.deep_link().on_open_url(move |event| {
                    let state = deep_link_handle.state::<sso::SsoState>();
                    for url in event.urls() {
                        sso::handle_deep_link(&state, &url);
                    }
                });
            }

            // --- システムトレイ ---
            let show_item = MenuItemBuilder::with_id("show", "表示").build(app)?;
            let stop_item = MenuItemBuilder::with_id("stop", "録画を停止").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "終了").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &stop_item, &quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip("MeetingRec - 待機中")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "stop" => {
                        app.state::<SessionManager>().request_stop();
                    }
                    "quit" => quit_gracefully(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // 自動起動（--minimized）時はトレイに常駐するだけにする
            let minimized = std::env::args().any(|a| a == MINIMIZED_ARG);
            if !minimized {
                show_main_window(&handle);
            }

            // --- バックグラウンドタスク ---
            let scheduler_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                scheduler::init_scheduler(scheduler_handle).await;
            });
            let upload_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                drive::retry_loop(upload_handle).await;
            });

            if ffmpeg::ffmpeg_path() == std::path::Path::new("ffmpeg") {
                warn!("Bundled ffmpeg.exe not found; falling back to ffmpeg on PATH");
            }

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

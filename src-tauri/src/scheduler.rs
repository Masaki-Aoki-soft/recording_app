use chrono::{Datelike, Local, NaiveTime};
use log::{error, info};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};

use crate::models::{Schedule, ScheduleTriggeredPayload, ScheduleType};

/// スケジューラーの状態（アプリ全体で共有）
pub struct SchedulerState {
    /// スケジュール再計算を通知するための Notify
    pub notify: Notify,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
        }
    }
}

/// 次にトリガーすべき時刻までの秒数を計算
fn seconds_until_next_trigger(schedule: &Schedule) -> Option<i64> {
    let now = Local::now();

    match &schedule.schedule_type {
        ScheduleType::Once { datetime } => {
            // ISO 8601 文字列をパース
            if let Ok(target) = datetime.parse::<chrono::DateTime<chrono::FixedOffset>>() {
                let target_local: chrono::DateTime<Local> = target.with_timezone(&Local);
                let diff = target_local.signed_duration_since(now).num_seconds();
                if diff > 0 {
                    Some(diff)
                } else {
                    None // 過去の日時
                }
            } else {
                error!("Failed to parse datetime: {}", datetime);
                None
            }
        }
        ScheduleType::Weekly {
            day_of_week,
            hour,
            minute,
        } => {
            let target_time = NaiveTime::from_hms_opt(*hour, *minute, 0)?;
            let now_weekday = now.weekday().num_days_from_sunday(); // 0=日曜
            let target_weekday = *day_of_week;

            // 今日の対象時刻
            let mut days_ahead = (target_weekday as i64) - (now_weekday as i64);
            if days_ahead < 0 {
                days_ahead += 7;
            }

            let target_date = now.date_naive() + chrono::Duration::days(days_ahead);
            let target_datetime = target_date.and_time(target_time);
            let target_local = target_datetime
                .and_local_timezone(Local)
                .single()?;

            let diff = target_local.signed_duration_since(now).num_seconds();
            if diff > 0 {
                Some(diff)
            } else {
                // 今週は過ぎた → 来週
                let next_week = target_date + chrono::Duration::days(7);
                let next_datetime = next_week.and_time(target_time);
                let next_local = next_datetime.and_local_timezone(Local).single()?;
                Some(next_local.signed_duration_since(now).num_seconds())
            }
        }
    }
}

/// スケジュール一覧をストアから読み込む
pub fn load_schedules(app: &AppHandle) -> Vec<Schedule> {
    let store = match app.store("schedules.json") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open schedule store: {}", e);
            return vec![];
        }
    };

    match store.get("schedules") {
        Some(val) => serde_json::from_value(val.clone()).unwrap_or_default(),
        None => vec![],
    }
}

/// スケジュール一覧をストアに保存
pub fn save_schedules(app: &AppHandle, schedules: &[Schedule]) {
    let store = match app.store("schedules.json") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open schedule store: {}", e);
            return;
        }
    };

    let val = serde_json::to_value(schedules).unwrap_or_default();
    store.set("schedules", val);
    if let Err(e) = store.save() {
        error!("Failed to save schedule store: {}", e);
    }
}

/// スケジューラーをバックグラウンドで起動
pub async fn init_scheduler(app_handle: AppHandle) {
    let handle = app_handle;

    info!("Scheduler started");

    loop {
            let schedules = load_schedules(&handle);
            let active_schedules: Vec<&Schedule> =
                schedules.iter().filter(|s| s.active).collect();

            if active_schedules.is_empty() {
                info!("No active schedules. Waiting for notification...");
                // アクティブなスケジュールがない場合、通知を待つ
                let state = handle.state::<Arc<SchedulerState>>();
                state.notify.notified().await;
                continue;
            }

            // 最も近い発火時刻を見つける
            let mut nearest_seconds: Option<i64> = None;
            let mut nearest_schedule: Option<&Schedule> = None;

            for schedule in &active_schedules {
                if let Some(secs) = seconds_until_next_trigger(schedule) {
                    if nearest_seconds.is_none() || secs < nearest_seconds.unwrap() {
                        nearest_seconds = Some(secs);
                        nearest_schedule = Some(schedule);
                    }
                }
            }

            match (nearest_seconds, nearest_schedule) {
                (Some(secs), Some(schedule)) => {
                    info!(
                        "Next schedule: '{}' in {} seconds",
                        schedule.name, secs
                    );

                    let schedule_clone = schedule.clone();
                    let state = handle.state::<Arc<SchedulerState>>();

                    // 正確な時刻まで待機、もしくは再計算通知を受ける
                    tokio::select! {
                        _ = sleep(Duration::from_secs(secs as u64)) => {
                            // 時刻到達: スケジュール実行
                            trigger_schedule(&handle, &schedule_clone).await;
                        }
                        _ = state.notify.notified() => {
                            // スケジュールが変更された → ループ先頭に戻って再計算
                            info!("Schedule changed, recalculating...");
                            continue;
                        }
                    }
                }
                _ => {
                    info!("No upcoming triggers found. Waiting for notification...");
                    let state = handle.state::<Arc<SchedulerState>>();
                    state.notify.notified().await;
                    continue;
                }
            }
    }
}

/// スケジュールを実行（URL起動 + フロントエンドにイベント発火）
async fn trigger_schedule(app: &AppHandle, schedule: &Schedule) {
    info!(
        "Triggering schedule: '{}' - Opening URL: {}",
        schedule.name, schedule.url
    );

    // 1. URLをデフォルトブラウザで開く（→ Zoom等のアプリが起動）
    if let Err(e) = tauri_plugin_opener::open_url(&schedule.url, None::<&str>) {
        error!("Failed to open URL: {}", e);
    }

    // 2. アプリ起動を待つ（10秒）
    sleep(Duration::from_secs(10)).await;

    // 3. フロントエンドにスケジュール発火イベントを送信
    let payload = ScheduleTriggeredPayload {
        schedule_id: schedule.id.clone(),
        schedule_name: schedule.name.clone(),
        url: schedule.url.clone(),
        duration_minutes: schedule.duration_minutes,
    };

    if let Err(e) = app.emit("schedule-triggered", &payload) {
        error!("Failed to emit schedule-triggered event: {}", e);
    }

    info!("Schedule triggered event emitted for '{}'", schedule.name);
}

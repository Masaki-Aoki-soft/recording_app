use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveTime};
use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};

use crate::models::{GeneralSettings, Schedule, ScheduleType};
use crate::session::{self, SessionKind};
use crate::settings;
use crate::AppFlags;

/// 1 回のスリープの最大時間。PC のスリープ復帰などで時刻がずれても、この間隔で再計算する
const MAX_TICK: Duration = Duration::from_secs(30);
/// 録画時間が未指定の場合に、開始時刻を過ぎてからでも参加する猶予
const DEFAULT_GRACE_MINUTES: u32 = 15;

/// スケジューラーの状態（アプリ全体で共有）
pub struct SchedulerState {
    /// スケジュール再計算を通知するための Notify
    pub notify: Notify,
    /// 発火済みの回（スケジュール ID → 開始日時）。同じ回を二重に実行しないため
    fired: Mutex<HashMap<String, DateTime<Local>>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            fired: Mutex::new(HashMap::new()),
        }
    }
}

/// 開始時刻を過ぎていても参加する猶予
pub fn grace_period(schedule: &Schedule) -> ChronoDuration {
    ChronoDuration::minutes(schedule.duration_minutes.unwrap_or(DEFAULT_GRACE_MINUTES).max(1) as i64)
}

/// 「進行中（開始から grace 以内）」または「これから始まる」直近の回の開始日時を返す
pub fn current_or_next_occurrence(
    schedule_type: &ScheduleType,
    now: DateTime<Local>,
    grace: ChronoDuration,
) -> Option<DateTime<Local>> {
    match schedule_type {
        ScheduleType::Once { datetime } => {
            let target = match datetime.parse::<DateTime<chrono::FixedOffset>>() {
                Ok(t) => t.with_timezone(&Local),
                Err(_) => {
                    error!("Failed to parse datetime: {}", datetime);
                    return None;
                }
            };
            (target + grace > now).then_some(target)
        }
        ScheduleType::Weekly {
            day_of_week,
            hour,
            minute,
        } => {
            let target_time = NaiveTime::from_hms_opt(*hour, *minute, 0)?;
            let now_weekday = now.weekday().num_days_from_sunday() as i64; // 0=日曜
            let days_ahead = (*day_of_week as i64 - now_weekday).rem_euclid(7);
            let base = now.date_naive() + ChronoDuration::days(days_ahead);

            // 日付を跨ぐ猶予（例: 23:50 開始）も考慮して前週・今週・翌週の候補を見る
            [-7i64, 0, 7].into_iter().find_map(|offset| {
                let date = base + ChronoDuration::days(offset);
                let start = date.and_time(target_time).and_local_timezone(Local).earliest()?;
                (start + grace > now).then_some(start)
            })
        }
    }
}

/// 一覧表示用: 次回の開始日時（進行中の回は含めない）
pub fn next_run(schedule: &Schedule, now: DateTime<Local>) -> Option<DateTime<Local>> {
    current_or_next_occurrence(&schedule.schedule_type, now, ChronoDuration::zero())
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
        Some(val) => serde_json::from_value(val).unwrap_or_default(),
        None => vec![],
    }
}

/// スケジュール一覧をストアに保存し、スケジューラとフロントに変更を通知する
pub fn save_schedules(app: &AppHandle, schedules: &[Schedule]) -> Result<(), String> {
    let store = app
        .store("schedules.json")
        .map_err(|e| format!("スケジュールストアを開けません: {}", e))?;

    let val = serde_json::to_value(schedules).map_err(|e| e.to_string())?;
    store.set("schedules", val);
    store
        .save()
        .map_err(|e| format!("スケジュールを保存できません: {}", e))?;

    app.state::<Arc<SchedulerState>>().notify.notify_one();
    let _ = app.emit("schedules-changed", ());
    Ok(())
}

/// スケジューラーをバックグラウンドで起動
pub async fn init_scheduler(app: AppHandle) {
    info!("Scheduler started");
    let state = app.state::<Arc<SchedulerState>>().inner().clone();

    loop {
        let wait = tick(&app, &state);
        tokio::select! {
            _ = sleep(wait) => {}
            _ = state.notify.notified() => {
                info!("Schedule changed, recalculating...");
            }
        }
    }
}

/// 期限が来たスケジュールを実行し、次に確認するまでの待ち時間を返す
fn tick(app: &AppHandle, state: &SchedulerState) -> Duration {
    let now = Local::now();
    let general: GeneralSettings = settings::load(app, settings::KEY_GENERAL_SETTINGS);
    let lead = ChronoDuration::seconds(general.lead_seconds as i64);

    let mut schedules = load_schedules(app);
    let mut wait = MAX_TICK;
    let mut due: Vec<(Schedule, DateTime<Local>)> = Vec::new();

    {
        let fired = state.fired.lock().unwrap();
        for schedule in schedules.iter().filter(|s| s.active) {
            let Some(start) =
                current_or_next_occurrence(&schedule.schedule_type, now, grace_period(schedule))
            else {
                continue;
            };
            if fired.get(&schedule.id) == Some(&start) {
                continue;
            }
            let trigger_at = start - lead;
            if now >= trigger_at {
                due.push((schedule.clone(), start));
            } else if let Ok(until) = (trigger_at - now).to_std() {
                wait = wait.min(until);
            }
        }
    }

    if due.is_empty() {
        return wait;
    }
    due.sort_by_key(|(_, start)| *start);

    let signed_in = app.state::<AppFlags>().is_signed_in();
    let mut deactivated = false;

    for (schedule, start) in due {
        state
            .fired
            .lock()
            .unwrap()
            .insert(schedule.id.clone(), start);

        if matches!(schedule.schedule_type, ScheduleType::Once { .. }) {
            if let Some(s) = schedules.iter_mut().find(|s| s.id == schedule.id) {
                s.active = false;
                deactivated = true;
            }
        }

        if !signed_in {
            warn!("Skipping schedule '{}': not signed in", schedule.name);
            continue;
        }

        info!("Triggering schedule '{}' (starts at {})", schedule.name, start);
        if let Err(e) = session::start(app, SessionKind::Scheduled(schedule.clone())) {
            warn!("Schedule '{}' was not started: {}", schedule.name, e);
        }
    }

    if deactivated {
        if let Err(e) = save_schedules(app, &schedules) {
            error!("{}", e);
        }
    }

    // 実行した直後は短い間隔で再確認
    Duration::from_secs(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    fn weekly(day_of_week: u32, hour: u32, minute: u32) -> ScheduleType {
        ScheduleType::Weekly {
            day_of_week,
            hour,
            minute,
        }
    }

    // 2026-09-30 は水曜日 (day_of_week = 3)
    const WED: u32 = 3;

    #[test]
    fn weekly_later_today() {
        let now = at(2026, 9, 30, 9, 0);
        let occ = current_or_next_occurrence(&weekly(WED, 10, 0), now, ChronoDuration::zero());
        assert_eq!(occ, Some(at(2026, 9, 30, 10, 0)));
    }

    #[test]
    fn weekly_in_progress_within_grace() {
        let now = at(2026, 9, 30, 10, 10);
        let occ = current_or_next_occurrence(&weekly(WED, 10, 0), now, ChronoDuration::minutes(15));
        assert_eq!(occ, Some(at(2026, 9, 30, 10, 0)));
    }

    #[test]
    fn weekly_past_grace_rolls_to_next_week() {
        let now = at(2026, 9, 30, 10, 20);
        let occ = current_or_next_occurrence(&weekly(WED, 10, 0), now, ChronoDuration::minutes(15));
        assert_eq!(occ, Some(at(2026, 10, 7, 10, 0)));
    }

    #[test]
    fn weekly_earlier_weekday_goes_to_next_week() {
        // 月曜 (1) の予定を水曜に計算すると翌週の月曜
        let now = at(2026, 9, 30, 9, 0);
        let occ = current_or_next_occurrence(&weekly(1, 9, 0), now, ChronoDuration::zero());
        assert_eq!(occ, Some(at(2026, 10, 5, 9, 0)));
    }

    #[test]
    fn weekly_grace_across_midnight() {
        // 火曜 23:50 開始の会議に、水曜 0:05 の時点で猶予 30 分なら進行中の回を返す
        let now = at(2026, 9, 30, 0, 5);
        let occ = current_or_next_occurrence(&weekly(2, 23, 50), now, ChronoDuration::minutes(30));
        assert_eq!(occ, Some(at(2026, 9, 29, 23, 50)));
    }

    #[test]
    fn once_future_past_and_grace() {
        let st = ScheduleType::Once {
            datetime: at(2026, 9, 30, 10, 0).to_rfc3339(),
        };
        let grace = ChronoDuration::minutes(15);
        assert!(current_or_next_occurrence(&st, at(2026, 9, 30, 9, 0), grace).is_some());
        assert!(current_or_next_occurrence(&st, at(2026, 9, 30, 10, 14), grace).is_some());
        assert!(current_or_next_occurrence(&st, at(2026, 9, 30, 10, 16), grace).is_none());
    }

    #[test]
    fn once_accepts_utc_iso_string() {
        // フロントは Date.toISOString()（UTC, Z 付き）で送ってくる
        let st = ScheduleType::Once {
            datetime: "2099-01-01T00:00:00.000Z".into(),
        };
        assert!(current_or_next_occurrence(&st, Local::now(), ChronoDuration::zero()).is_some());
    }
}

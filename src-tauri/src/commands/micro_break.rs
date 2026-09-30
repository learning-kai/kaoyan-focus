//! 随机微休息提示（专注中每隔一段随机时间提示闭眼休息几秒）。
//!
//! 设计要点：
//! - 提示时刻由后台 tick 判定，而不是前端定时器：主窗口隐藏到托盘后 WebView 的定时器会被降频，
//!   而后台 tick 每 3 秒运行一次，不受影响。前端只负责收到事件后放提示音、显示闭眼倒计时。
//! - 每轮专注的提示时刻表由 (学习模式 id, 轮次, 计时方式) 作为种子确定性生成，
//!   不写数据库；应用重启后同一轮会得到同一张时刻表，已经过去的提示不会补发。
//! - 微休息不改变学习模式状态机，也不暂停计时：闭眼 10 秒本身就是专注的一部分。
//! - 只在 `focus` 阶段、未暂停时提示；番茄钟最后 30 秒不再提示，避免和阶段结束提醒撞在一起。

use crate::{commands::focus::StudyModeState, storage::db::open_database};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub const MICRO_BREAK_CUE_EVENT: &str = "study-micro-break-cue";

const ENABLED_KEY: &str = "micro_break_enabled";
const MIN_INTERVAL_KEY: &str = "micro_break_min_interval_seconds";
const MAX_INTERVAL_KEY: &str = "micro_break_max_interval_seconds";
const REST_SECONDS_KEY: &str = "micro_break_rest_seconds";
const ADAPTIVE_KEY: &str = "micro_break_adaptive";
const VOLUME_KEY: &str = "micro_break_volume";

pub const MIN_INTERVAL_FLOOR_SECONDS: i64 = 60;
pub const MAX_INTERVAL_CEIL_SECONDS: i64 = 30 * 60;
pub const REST_SECONDS_MIN: i64 = 3;
pub const REST_SECONDS_MAX: i64 = 60;

/// 番茄钟本轮剩余不足这个时长时不再提示，让位给「番茄钟结束」提醒。
const PHASE_END_GUARD_SECONDS: i64 = 30;
/// 提示时刻过去超过这个时长才被发现（睡眠唤醒、应用刚启动），就静默跳过，不补发。
/// 后台 tick 间隔是 3 秒，留足余量。
const STALE_CUE_SECONDS: i64 = 10;
/// 时刻表最远生成到这里（12 小时，与正计时休息上限同量级），防止不限时长的正计时无限生成。
const SCHEDULE_HORIZON_CAP_SECONDS: i64 = 12 * 60 * 60;

/// 专注算法分段（秒）：前 30 分钟是进入心流的关键期，提示最密；
/// 30–60 分钟逐渐放宽，减少打断；60 分钟以后脑力下降，重新加密微休息。
const WARMUP_END_SECONDS: i64 = 30 * 60;
const DEEP_END_SECONDS: i64 = 60 * 60;
const WARMUP_FACTOR: f64 = 1.0;
const DEEP_FACTOR: f64 = 1.6;
const FATIGUE_FACTOR: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroBreakSettings {
    pub enabled: bool,
    pub min_interval_seconds: i64,
    pub max_interval_seconds: i64,
    pub rest_seconds: i64,
    /// 按专注算法随时长调整提示密度；关闭后始终在 [min, max] 内均匀随机。
    pub adaptive: bool,
    /// 提示音音量 0–100，独立于提醒铃声，默认更轻。
    pub volume: i64,
}

impl Default for MicroBreakSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_interval_seconds: 3 * 60,
            max_interval_seconds: 5 * 60,
            rest_seconds: 10,
            adaptive: true,
            volume: 60,
        }
    }
}

impl MicroBreakSettings {
    pub fn normalized(self) -> Self {
        let mut min = self
            .min_interval_seconds
            .clamp(MIN_INTERVAL_FLOOR_SECONDS, MAX_INTERVAL_CEIL_SECONDS);
        let mut max = self
            .max_interval_seconds
            .clamp(MIN_INTERVAL_FLOOR_SECONDS, MAX_INTERVAL_CEIL_SECONDS);
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        Self {
            enabled: self.enabled,
            min_interval_seconds: min,
            max_interval_seconds: max,
            rest_seconds: self.rest_seconds.clamp(REST_SECONDS_MIN, REST_SECONDS_MAX),
            adaptive: self.adaptive,
            volume: self.volume.clamp(0, 100),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MicroBreakCue {
    pub study_mode_id: i64,
    pub cycle_index: i64,
    /// 本轮第几次提示（从 1 开始）。
    pub cue_number: usize,
    pub rest_seconds: i64,
    pub volume: i64,
    pub phase_elapsed_seconds: i64,
}

// ---------------------------------------------------------------------------
// 时刻表
// ---------------------------------------------------------------------------

/// splitmix64：实现简单、输出稳定，不依赖 rand crate 的版本行为。
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// [low, high] 闭区间内的整数。
    fn range_inclusive(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = (high - low + 1) as u64;
        low + (self.next() % span) as i64
    }
}

pub fn schedule_seed(study_mode_id: i64, cycle_index: i64, timer_kind: &str) -> u64 {
    let kind_salt: u64 = if timer_kind == "countup" {
        0xC0FF_EE00
    } else {
        0x0701_1A70
    };
    (study_mode_id as u64)
        .wrapping_mul(0x1000_0000_01B3)
        .wrapping_add((cycle_index as u64).wrapping_mul(0x9E37_79B9))
        ^ kind_salt
}

/// 下一次提示间隔的密度系数，由「上一次提示发生时已专注多久」决定。
pub fn density_factor(elapsed_seconds: i64, adaptive: bool) -> f64 {
    if !adaptive {
        return 1.0;
    }
    if elapsed_seconds < WARMUP_END_SECONDS {
        WARMUP_FACTOR
    } else if elapsed_seconds < DEEP_END_SECONDS {
        DEEP_FACTOR
    } else {
        FATIGUE_FACTOR
    }
}

/// 生成本轮专注的提示时刻（相对本轮开始的秒数，严格递增）。
///
/// `run_seconds` 为本轮专注总长；`None` 表示不限时长（正计时），生成到 12 小时为止。
pub fn build_cue_schedule(
    settings: &MicroBreakSettings,
    seed: u64,
    run_seconds: Option<i64>,
) -> Vec<i64> {
    let settings = settings.normalized();
    let horizon = match run_seconds {
        Some(run) => (run - PHASE_END_GUARD_SECONDS).min(SCHEDULE_HORIZON_CAP_SECONDS),
        None => SCHEDULE_HORIZON_CAP_SECONDS,
    };
    let mut rng = SplitMix64(seed);
    let mut cues = Vec::new();
    let mut at = 0_i64;
    loop {
        let factor = density_factor(at, settings.adaptive);
        let low = ((settings.min_interval_seconds as f64) * factor).round() as i64;
        let high = ((settings.max_interval_seconds as f64) * factor).round() as i64;
        let low = low.max(MIN_INTERVAL_FLOOR_SECONDS);
        let high = high.max(low);
        at += rng.range_inclusive(low, high);
        if at > horizon {
            break;
        }
        cues.push(at);
    }
    cues
}

// ---------------------------------------------------------------------------
// 触发判定
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct CueTrackerKey {
    study_mode_id: i64,
    cycle_index: i64,
    timer_kind: String,
}

#[derive(Debug, Default)]
struct CueTracker {
    key: Option<CueTrackerKey>,
    /// 本轮已经处理（发出或跳过）的提示个数。
    handled: usize,
}

static TRACKER: Mutex<CueTracker> = Mutex::new(CueTracker {
    key: None,
    handled: 0,
});

/// 纯函数：给定已处理个数和当前进度，决定要不要发提示。
///
/// 返回 (新的已处理个数, 要发出的提示序号 1-based)。
/// 一次只发一个；跨过多个提示（例如睡眠唤醒）时只看最后一个，且过期就不发。
fn decide_cue(cues: &[i64], handled: usize, elapsed: i64) -> (usize, Option<usize>) {
    let passed = cues.partition_point(|at| *at <= elapsed);
    if passed < handled {
        // 进度倒退（切换计时方式等导致本轮重新计时），按当前进度重新对齐，不补发。
        return (passed, None);
    }
    if passed == handled {
        return (handled, None);
    }
    let latest_at = cues[passed - 1];
    if elapsed - latest_at <= STALE_CUE_SECONDS {
        (passed, Some(passed))
    } else {
        (passed, None)
    }
}

fn focus_run_seconds(state: &StudyModeState) -> Option<i64> {
    if state.timer_kind == "countup" && state.planned_seconds <= 0 {
        None
    } else {
        Some(state.phase_elapsed_seconds + state.phase_remaining_seconds)
    }
}

/// 后台 tick 调用：判断本次 tick 是否需要发出微休息提示。
pub fn tick_micro_break(app: &AppHandle, state: &StudyModeState) -> Result<(), String> {
    let running_focus = state.status == "active" && state.phase == "focus" && !state.is_paused;
    let Some(study_mode_id) = state.id.filter(|_| running_focus) else {
        return Ok(());
    };

    let settings = load_settings(app)?;
    if !settings.enabled {
        return Ok(());
    }

    let key = CueTrackerKey {
        study_mode_id,
        cycle_index: state.cycle_index,
        timer_kind: state.timer_kind.clone(),
    };
    let cues = build_cue_schedule(
        &settings,
        schedule_seed(study_mode_id, state.cycle_index, &state.timer_kind),
        focus_run_seconds(state),
    );

    let cue_number = {
        let mut tracker = TRACKER.lock().map_err(|error| error.to_string())?;
        if tracker.key.as_ref() != Some(&key) {
            // 新的一轮（或应用刚启动）：已经过去的提示视为已处理，只有刚刚到点的会在下面发出。
            tracker.key = Some(key);
            tracker.handled =
                cues.partition_point(|at| *at <= state.phase_elapsed_seconds - STALE_CUE_SECONDS);
        }
        let (handled, emit) = decide_cue(&cues, tracker.handled, state.phase_elapsed_seconds);
        tracker.handled = handled;
        emit
    };

    if let Some(cue_number) = cue_number {
        let cue = MicroBreakCue {
            study_mode_id,
            cycle_index: state.cycle_index,
            cue_number,
            rest_seconds: settings.rest_seconds,
            volume: settings.volume,
            phase_elapsed_seconds: state.phase_elapsed_seconds,
        };
        app.emit(MICRO_BREAK_CUE_EVENT, cue)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 设置读写
// ---------------------------------------------------------------------------

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| error.to_string())
}

fn read_i64(connection: &Connection, key: &str, fallback: i64) -> Result<i64, String> {
    Ok(read_setting(connection, key)?
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .unwrap_or(fallback))
}

fn read_bool(connection: &Connection, key: &str, fallback: bool) -> Result<bool, String> {
    Ok(
        match read_setting(connection, key)?
            .map(|raw| raw.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("1" | "true" | "yes" | "on") => true,
            Some("0" | "false" | "no" | "off") => false,
            _ => fallback,
        },
    )
}

fn write_setting(connection: &Connection, key: &str, value: &str, now: &str) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![key, value, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_settings_from(connection: &Connection) -> Result<MicroBreakSettings, String> {
    let defaults = MicroBreakSettings::default();
    Ok(MicroBreakSettings {
        enabled: read_bool(connection, ENABLED_KEY, defaults.enabled)?,
        min_interval_seconds: read_i64(
            connection,
            MIN_INTERVAL_KEY,
            defaults.min_interval_seconds,
        )?,
        max_interval_seconds: read_i64(
            connection,
            MAX_INTERVAL_KEY,
            defaults.max_interval_seconds,
        )?,
        rest_seconds: read_i64(connection, REST_SECONDS_KEY, defaults.rest_seconds)?,
        adaptive: read_bool(connection, ADAPTIVE_KEY, defaults.adaptive)?,
        volume: read_i64(connection, VOLUME_KEY, defaults.volume)?,
    }
    .normalized())
}

fn load_settings(app: &AppHandle) -> Result<MicroBreakSettings, String> {
    let connection = open_database(&database_path(app)?)?;
    load_settings_from(&connection)
}

#[tauri::command]
pub fn get_micro_break_settings(app: AppHandle) -> Result<MicroBreakSettings, String> {
    load_settings(&app)
}

/// 微休息只是提示，不是约束，因此学习模式运行中也允许修改。
#[tauri::command]
pub fn save_micro_break_settings(
    app: AppHandle,
    settings: MicroBreakSettings,
) -> Result<MicroBreakSettings, String> {
    let normalized = settings.normalized();
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();
    let flag = |value: bool| if value { "1" } else { "0" };
    write_setting(&connection, ENABLED_KEY, flag(normalized.enabled), &now)?;
    write_setting(
        &connection,
        MIN_INTERVAL_KEY,
        &normalized.min_interval_seconds.to_string(),
        &now,
    )?;
    write_setting(
        &connection,
        MAX_INTERVAL_KEY,
        &normalized.max_interval_seconds.to_string(),
        &now,
    )?;
    write_setting(
        &connection,
        REST_SECONDS_KEY,
        &normalized.rest_seconds.to_string(),
        &now,
    )?;
    write_setting(&connection, ADAPTIVE_KEY, flag(normalized.adaptive), &now)?;
    write_setting(
        &connection,
        VOLUME_KEY,
        &normalized.volume.to_string(),
        &now,
    )?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(min: i64, max: i64, adaptive: bool) -> MicroBreakSettings {
        MicroBreakSettings {
            enabled: true,
            min_interval_seconds: min,
            max_interval_seconds: max,
            adaptive,
            ..MicroBreakSettings::default()
        }
    }

    #[test]
    fn normalized_clamps_and_orders_range() {
        let normalized = MicroBreakSettings {
            min_interval_seconds: 5000,
            max_interval_seconds: 10,
            rest_seconds: 0,
            volume: 400,
            ..MicroBreakSettings::default()
        }
        .normalized();
        assert_eq!(normalized.min_interval_seconds, MIN_INTERVAL_FLOOR_SECONDS);
        assert_eq!(normalized.max_interval_seconds, MAX_INTERVAL_CEIL_SECONDS);
        assert_eq!(normalized.rest_seconds, REST_SECONDS_MIN);
        assert_eq!(normalized.volume, 100);
    }

    #[test]
    fn fixed_schedule_stays_within_range_and_is_deterministic() {
        let s = settings(180, 300, false);
        let seed = schedule_seed(42, 1, "pomodoro");
        let cues = build_cue_schedule(&s, seed, Some(90 * 60));
        assert_eq!(
            cues,
            build_cue_schedule(&s, seed, Some(90 * 60)),
            "同一种子必须得到同一张时刻表"
        );
        assert!(!cues.is_empty());
        let mut previous = 0;
        for at in &cues {
            let gap = at - previous;
            assert!((180..=300).contains(&gap), "间隔 {gap} 超出 3–5 分钟");
            previous = *at;
        }
        assert!(*cues.last().unwrap() <= 90 * 60 - PHASE_END_GUARD_SECONDS);
        // 90 分钟、每 3–5 分钟一次，应在 18–30 次之间。
        assert!((17..=30).contains(&cues.len()), "次数 {}", cues.len());
    }

    #[test]
    fn different_rounds_get_different_schedules() {
        let s = settings(180, 300, true);
        let a = build_cue_schedule(&s, schedule_seed(7, 1, "pomodoro"), Some(90 * 60));
        let b = build_cue_schedule(&s, schedule_seed(7, 2, "pomodoro"), Some(90 * 60));
        let c = build_cue_schedule(&s, schedule_seed(8, 1, "pomodoro"), Some(90 * 60));
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn adaptive_schedule_follows_focus_algorithm() {
        let s = settings(180, 300, true);
        for seed in 0..50_u64 {
            let cues = build_cue_schedule(&s, seed, Some(120 * 60));
            let mut previous = 0;
            for at in &cues {
                let gap = at - previous;
                let factor = density_factor(previous, true);
                let low = (180.0 * factor).round() as i64;
                let high = (300.0 * factor).round() as i64;
                assert!(
                    gap >= low && gap <= high,
                    "seed {seed}: {previous}s 后间隔 {gap} 不在 [{low}, {high}]"
                );
                previous = *at;
            }
        }
        assert_eq!(density_factor(10 * 60, true), WARMUP_FACTOR);
        assert!(
            density_factor(45 * 60, true) > WARMUP_FACTOR,
            "30–60 分钟应放宽"
        );
        assert!(
            density_factor(75 * 60, true) < density_factor(45 * 60, true),
            "60 分钟后应重新加密"
        );
        assert_eq!(density_factor(75 * 60, false), 1.0);
    }

    #[test]
    fn short_pomodoro_may_have_no_cues_and_countup_is_capped() {
        let s = settings(180, 300, false);
        assert!(
            build_cue_schedule(&s, 1, Some(120)).is_empty(),
            "2 分钟的番茄钟不够一次提示"
        );
        let countup = build_cue_schedule(&s, 1, None);
        assert!(*countup.last().unwrap() <= SCHEDULE_HORIZON_CAP_SECONDS);
    }

    #[test]
    fn decide_cue_emits_once_per_cue() {
        let cues = vec![200, 450, 700];
        assert_eq!(decide_cue(&cues, 0, 150), (0, None));
        assert_eq!(decide_cue(&cues, 0, 201), (1, Some(1)), "刚到点就发");
        assert_eq!(decide_cue(&cues, 1, 204), (1, None), "同一个提示不重复发");
        assert_eq!(decide_cue(&cues, 1, 452), (2, Some(2)));
    }

    #[test]
    fn decide_cue_skips_stale_and_handles_rewind() {
        let cues = vec![200, 450, 700];
        // 睡眠唤醒后一次跨过两个提示，且最后一个已经过期：静默对齐，不补发。
        assert_eq!(decide_cue(&cues, 0, 600), (2, None));
        // 跨过两个提示但最后一个刚到点：只发最后一个。
        assert_eq!(decide_cue(&cues, 0, 455), (2, Some(2)));
        // 进度倒退：重新对齐，不发。
        assert_eq!(decide_cue(&cues, 3, 100), (0, None));
    }
}

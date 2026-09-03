use crate::{
    focus::{session::FocusSession, subject::Subject},
    storage::db::open_database,
    sync_package::{load_or_create_device_id, mark_entity_deleted},
    AppState,
};
use chrono::{DateTime, Datelike, Duration, FixedOffset, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::thread;
use tauri::{AppHandle, Manager, State};

const MIN_RECORDED_FOCUS_SECONDS: i64 = 60;
/// 日历时间轴一次最多渲染的专注色带条数，防止异常数据把界面撑爆。
const MAX_FOCUS_SESSIONS_IN_RANGE: usize = 500;
const CONTROL_PAUSE: &str = "pause";
const CONTROL_RESUME: &str = "resume";
const CONTROL_CONFIRM_BREAK: &str = "confirm_break";
const CONTROL_SKIP_BREAK: &str = "skip_break";
const CONTROL_FINISH: &str = "finish";
const CONTROL_EMERGENCY_EXIT: &str = "emergency_exit";
const CONTROL_SWITCH_SUBJECT: &str = "switch_subject";
const CONTROL_SWITCH_TIMER_KIND: &str = "switch_timer_kind";
const CONTROL_MANUAL_BREAK: &str = "manual_break";
const PRIMARY_OWNER_DEVICE_ID_KEY: &str = "primary_owner_device_id";

pub(crate) const TIMER_KIND_POMODORO: &str = "pomodoro";
pub(crate) const TIMER_KIND_COUNTUP: &str = "countup";

/// 正计时模式休息时长允许范围：1–60 分钟（含预设 5/10/15/20 与手动填写）。
pub(crate) const COUNTUP_BREAK_MIN_SECONDS: i64 = 60;
pub(crate) const COUNTUP_BREAK_MAX_SECONDS: i64 = 3600;

#[derive(Debug, Clone, Serialize)]
pub struct FocusStatsSummary {
    pub today_seconds: i64,
    pub week_seconds: i64,
    pub month_seconds: i64,
    pub interruption_count: i64,
    pub subjects: Vec<SubjectStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectStats {
    pub subject: Subject,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSessionRecovery {
    pub recovery_status: String,
    pub session: FocusSession,
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StudyModeLinks {
    pub schedule_block_id: Option<i64>,
    pub today_plan_item_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudyModeState {
    pub id: Option<i64>,
    pub state_revision: Option<i64>,
    pub phase: String,
    pub status: String,
    pub mode: String,
    pub timer_kind: String,
    pub subject_id: Option<i64>,
    pub planned_seconds: i64,
    pub focus_seconds: i64,
    pub break_seconds: i64,
    pub long_break_seconds: i64,
    pub long_break_interval: i64,
    pub effective_break_seconds: i64,
    pub break_kind: String,
    pub cycle_index: i64,
    pub started_at: Option<String>,
    pub phase_started_at: Option<String>,
    pub paused_at: Option<String>,
    pub ended_at: Option<String>,
    pub current_session: Option<FocusSession>,
    pub study_elapsed_seconds: i64,
    pub study_remaining_seconds: i64,
    pub phase_elapsed_seconds: i64,
    pub phase_remaining_seconds: i64,
    pub focus_enforcement_active: bool,
    pub whitelist_enabled: bool,
    pub is_paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StudyRuntimeSyncMarker {
    id: Option<i64>,
    state_revision: i64,
    phase: String,
    status: String,
    subject_id: Option<i64>,
    cycle_index: i64,
    paused_at: Option<String>,
    current_session_id: Option<i64>,
    break_kind: String,
    timer_kind: String,
}

#[derive(Debug, Clone)]
struct StudyModeRecord {
    id: i64,
    state_revision: i64,
    mode: String,
    timer_kind: String,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    whitelist_enabled: bool,
    phase: String,
    cycle_index: i64,
    started_at: String,
    phase_started_at: String,
    paused_at: Option<String>,
    accumulated_study_seconds: i64,
    _total_paused_seconds: i64,
    _phase_paused_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
    schedule_block_id: Option<i64>,
    _today_plan_item_id: Option<i64>,
    status: String,
}


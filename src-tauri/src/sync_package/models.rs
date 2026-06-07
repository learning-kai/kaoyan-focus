use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

const SYNC_SCHEMA_VERSION: i64 = 2;
const ENTITY_SUBJECT: &str = "subject";
const ENTITY_STUDY_MODE: &str = "study_mode";
const ENTITY_FOCUS_SESSION: &str = "focus_session";
const ENTITY_APP_EVENT: &str = "app_event";
const ENTITY_CHECKLIST_TASK: &str = "checklist_task";
const ENTITY_TODAY_PLAN_ITEM: &str = "today_plan_item";
const ENTITY_SCHEDULE_BLOCK: &str = "schedule_block";
const ENTITY_SCHEDULE_TEMPLATE: &str = "schedule_template";
const ENTITY_DAILY_REVIEW: &str = "daily_review";
const ENTITY_WEEKLY_REVIEW: &str = "weekly_review";
const DEFAULT_SUBJECT_SYNC_IDS: [&str; 4] = ["subject-1", "subject-2", "subject-3", "subject-4"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSyncPayload {
    pub schema_version: i64,
    pub device_id: String,
    pub exported_at: i64,
    #[serde(default)]
    pub source_device_id: Option<String>,
    #[serde(default)]
    pub active_device_id: Option<String>,
    #[serde(default)]
    pub primary_owner_device_id: Option<String>,
    #[serde(default)]
    pub primary_owner_updated_at: Option<i64>,
    #[serde(default)]
    pub subjects: Vec<SharedSubject>,
    #[serde(default)]
    pub study_modes: Vec<SharedStudyMode>,
    #[serde(default)]
    pub focus_sessions: Vec<SharedFocusSession>,
    #[serde(default)]
    pub app_events: Vec<SharedAppEvent>,
    #[serde(default)]
    pub checklist_tasks: Vec<SharedChecklistTask>,
    #[serde(default)]
    pub today_plan_items: Vec<SharedTodayPlanItem>,
    #[serde(default)]
    pub schedule_blocks: Vec<SharedScheduleBlock>,
    #[serde(default)]
    pub schedule_templates: Vec<SharedScheduleTemplate>,
    #[serde(default)]
    pub daily_reviews: Vec<SharedDailyReview>,
    #[serde(default)]
    pub weekly_reviews: Vec<SharedWeeklyReview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedActiveStudySnapshot {
    pub sync_id: String,
    pub status: Option<String>,
    pub phase: Option<String>,
    pub subject_sync_id: Option<String>,
    pub current_session_sync_id: Option<String>,
    pub phase_started_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub round_number: Option<i64>,
    pub current_break_type: Option<String>,
    pub ended_at: Option<i64>,
    pub state_revision: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSubject {
    pub sync_id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub enabled: Option<bool>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedStudyMode {
    pub sync_id: String,
    #[serde(default)]
    pub state_revision: Option<i64>,
    pub mode: Option<String>,
    pub subject_sync_id: Option<String>,
    pub planned_seconds: Option<i64>,
    pub focus_seconds: Option<i64>,
    pub break_seconds: Option<i64>,
    pub long_break_seconds: Option<i64>,
    pub long_break_interval: Option<i64>,
    pub phase: Option<String>,
    pub round_number: Option<i64>,
    pub started_at: Option<i64>,
    pub phase_started_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub paused_from_phase: Option<String>,
    pub accumulated_study_seconds: Option<i64>,
    pub total_paused_seconds: Option<i64>,
    pub phase_paused_seconds: Option<i64>,
    pub paused_stage_elapsed_seconds: Option<i64>,
    pub current_break_type: Option<String>,
    pub ended_at: Option<i64>,
    pub current_session_sync_id: Option<String>,
    pub schedule_block_sync_id: Option<String>,
    pub today_plan_item_sync_id: Option<String>,
    #[serde(default)]
    pub last_control_device_id: Option<String>,
    #[serde(default)]
    pub last_control_action: Option<String>,
    #[serde(default)]
    pub last_control_at: Option<i64>,
    pub status: Option<String>,
    pub finish_reason: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFocusSession {
    pub sync_id: String,
    pub study_mode_sync_id: Option<String>,
    pub subject_sync_id: Option<String>,
    pub mode: Option<String>,
    pub planned_seconds: Option<i64>,
    pub actual_seconds: Option<i64>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub status: Option<String>,
    pub end_reason: Option<String>,
    pub interruption_count: Option<i64>,
    pub emergency_exit_count: Option<i64>,
    pub paused_seconds: Option<i64>,
    pub followed_by_break_type: Option<String>,
    pub schedule_block_sync_id: Option<String>,
    pub today_plan_item_sync_id: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveRemoteMergeDecision {
    Default,
    KeepLocalActive,
    AcceptRemoteCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedAppEvent {
    pub sync_id: String,
    pub study_mode_sync_id: Option<String>,
    pub focus_session_sync_id: Option<String>,
    pub package_name: Option<String>,
    pub app_name: Option<String>,
    pub event_type: Option<String>,
    pub action: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedChecklistTask {
    pub sync_id: String,
    pub category_key: Option<String>,
    pub subject_sync_id: Option<String>,
    pub title: Option<String>,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: Option<f64>,
    pub completed: Option<bool>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedTodayPlanItem {
    pub sync_id: String,
    pub today_date: Option<String>,
    pub source_task_sync_id: Option<String>,
    pub subject_sync_id: Option<String>,
    pub title: Option<String>,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: Option<f64>,
    pub completed: Option<bool>,
    pub synced_source_completion: Option<bool>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedScheduleBlock {
    pub sync_id: String,
    pub schedule_date: Option<String>,
    pub title: Option<String>,
    pub note: Option<String>,
    pub category_key: Option<String>,
    pub subject_sync_id: Option<String>,
    pub source_today_item_sync_id: Option<String>,
    pub template_sync_id: Option<String>,
    pub start_minute: Option<i64>,
    pub end_minute: Option<i64>,
    pub status: Option<String>,
    pub linked_study_mode_sync_id: Option<String>,
    pub linked_focus_session_sync_id: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedScheduleTemplate {
    pub sync_id: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub category_key: Option<String>,
    pub subject_sync_id: Option<String>,
    pub weekdays: Option<Vec<i64>>,
    pub start_minute: Option<i64>,
    pub end_minute: Option<i64>,
    pub enabled: Option<bool>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedDailyReview {
    pub sync_id: String,
    pub review_date: Option<String>,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub tomorrow_focus: Option<String>,
    pub mood_score: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedWeeklyReview {
    pub sync_id: String,
    pub week_start_date: Option<String>,
    pub summary: Option<String>,
    pub blockers: Option<String>,
    pub next_week_focus: Option<String>,
    pub mood_score: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct SyncMetaRow {
    local_id: i64,
    sync_id: String,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct DesktopSubjectRow {
    id: i64,
    name: String,
    color: Option<String>,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopStudyModeRow {
    id: i64,
    state_revision: i64,
    mode: String,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    phase: String,
    cycle_index: i64,
    started_at: String,
    phase_started_at: String,
    paused_at: Option<String>,
    total_paused_seconds: i64,
    _phase_paused_seconds: i64,
    accumulated_study_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    last_control_device_id: Option<String>,
    last_control_action: Option<String>,
    last_control_at: Option<i64>,
    status: String,
    finish_reason: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopFocusSessionRow {
    id: i64,
    mode: String,
    subject_id: Option<i64>,
    planned_seconds: i64,
    actual_seconds: i64,
    started_at: String,
    ended_at: Option<String>,
    status: String,
    end_reason: Option<String>,
    interruption_count: i64,
    emergency_exit_count: i64,
    paused_seconds: i64,
    followed_by_break_type: Option<String>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopAppEventRow {
    id: i64,
    session_id: i64,
    process_name: String,
    window_title: Option<String>,
    event_type: String,
    action_taken: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct DesktopChecklistTaskRow {
    id: i64,
    board_scope: String,
    subject_id: Option<i64>,
    title: String,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopTodayPlanItemRow {
    id: i64,
    today_date: String,
    source_task_id: Option<i64>,
    subject_id: Option<i64>,
    title: String,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    synced_source_completion: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopScheduleBlockRow {
    id: i64,
    schedule_date: String,
    title: String,
    note: Option<String>,
    category_key: String,
    subject_id: Option<i64>,
    source_today_item_id: Option<i64>,
    template_id: Option<i64>,
    start_minute: i64,
    end_minute: i64,
    status: String,
    linked_study_mode_id: Option<i64>,
    linked_focus_session_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopScheduleTemplateRow {
    id: i64,
    title: String,
    note: Option<String>,
    category_key: String,
    subject_id: Option<i64>,
    weekdays: String,
    start_minute: i64,
    end_minute: i64,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopDailyReviewRow {
    id: i64,
    review_date: String,
    summary: Option<String>,
    blockers: Option<String>,
    tomorrow_focus: Option<String>,
    mood_score: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct DesktopWeeklyReviewRow {
    id: i64,
    week_start_date: String,
    summary: Option<String>,
    blockers: Option<String>,
    next_week_focus: Option<String>,
    mood_score: i64,
    created_at: String,
    updated_at: String,
}


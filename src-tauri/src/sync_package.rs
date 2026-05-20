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

pub fn export_shared_sync_payload(
    connection: &Connection,
    device_id: String,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let mut payload = SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION,
        device_id: device_id.clone(),
        exported_at,
        source_device_id: Some(device_id.clone()),
        active_device_id: active_device_id(connection).ok().flatten(),
        subjects: export_subjects(connection)?,
        study_modes: export_study_modes(connection)?,
        focus_sessions: export_focus_sessions(connection)?,
        app_events: export_app_events(connection)?,
        checklist_tasks: export_checklist_tasks(connection)?,
        today_plan_items: export_today_plan_items(connection)?,
        schedule_blocks: export_schedule_blocks(connection)?,
        schedule_templates: export_schedule_templates(connection)?,
        daily_reviews: export_daily_reviews(connection)?,
        weekly_reviews: export_weekly_reviews(connection)?,
    };
    canonicalize_subject_payload(&mut payload);
    Ok(payload)
}

pub fn merge_shared_sync_payloads(
    mut local: SharedSyncPayload,
    mut remote: SharedSyncPayload,
    device_id: String,
    exported_at: i64,
) -> SharedSyncPayload {
    canonicalize_subject_payload(&mut local);
    canonicalize_subject_payload(&mut remote);
    let local_active = shared_active_study_snapshot(&local);
    let remote_active = shared_active_study_snapshot(&remote);
    let active_merge_decision =
        classify_active_remote_merge(&local, &remote, local_active.as_ref(), exported_at);
    let keep_local_active = should_keep_local_active(
        &local,
        &remote,
        local_active.as_ref(),
        remote_active.as_ref(),
        exported_at,
    ) || active_merge_decision
        == ActiveRemoteMergeDecision::KeepLocalActive;
    let preferred_active_sync_id = match (&local_active, &remote_active) {
        (Some(local_snapshot), Some(_)) if keep_local_active => {
            Some(local_snapshot.sync_id.clone())
        }
        _ => None,
    };
    let mut study_modes = merge_study_modes(&local.study_modes, &remote.study_modes);
    let mut focus_sessions = merge_focus_sessions(&local.focus_sessions, &remote.focus_sessions);
    resolve_shared_active_conflicts(
        &mut study_modes,
        &mut focus_sessions,
        exported_at,
        preferred_active_sync_id.as_deref(),
    );
    if keep_local_active {
        if let Some(snapshot) = local_active.as_ref() {
            restore_active_from_payload(&mut study_modes, &mut focus_sessions, &local, snapshot);
        }
    } else if active_merge_decision == ActiveRemoteMergeDecision::AcceptRemoteCommand {
        restore_matching_active_from_remote(
            &mut study_modes,
            &mut focus_sessions,
            &remote,
            local_active.as_ref(),
        );
    }

    SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION
            .max(local.schema_version)
            .max(remote.schema_version),
        device_id: device_id.clone(),
        exported_at,
        source_device_id: Some(device_id.clone()),
        active_device_id: shared_active_study_snapshot_from_modes(&study_modes).and_then(
            |snapshot| {
                if local.active_device_id.is_some()
                    && local.device_id == device_id
                    && local
                        .study_modes
                        .iter()
                        .any(|mode| mode.sync_id == snapshot.sync_id)
                {
                    local.active_device_id.clone()
                } else if remote.active_device_id.is_some()
                    && remote
                        .study_modes
                        .iter()
                        .any(|mode| mode.sync_id == snapshot.sync_id)
                {
                    remote.active_device_id.clone()
                } else {
                    Some(device_id.clone())
                }
            },
        ),
        subjects: merge_latest_by_sync_id(
            &local.subjects,
            &remote.subjects,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        study_modes,
        focus_sessions,
        app_events: merge_latest_by_sync_id(
            &local.app_events,
            &remote.app_events,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        checklist_tasks: merge_latest_by_sync_id(
            &local.checklist_tasks,
            &remote.checklist_tasks,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        today_plan_items: merge_latest_by_sync_id(
            &local.today_plan_items,
            &remote.today_plan_items,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        schedule_blocks: merge_latest_by_sync_id(
            &local.schedule_blocks,
            &remote.schedule_blocks,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        schedule_templates: merge_latest_by_sync_id(
            &local.schedule_templates,
            &remote.schedule_templates,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        daily_reviews: merge_latest_by_sync_id(
            &local.daily_reviews,
            &remote.daily_reviews,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        weekly_reviews: merge_latest_by_sync_id(
            &local.weekly_reviews,
            &remote.weekly_reviews,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
    }
}

pub fn merge_remote_payload_into_local(
    local: SharedSyncPayload,
    remote: SharedSyncPayload,
    device_id: String,
    exported_at: i64,
) -> SharedSyncPayload {
    let mut local = local;
    let mut remote = remote;
    canonicalize_subject_payload(&mut local);
    canonicalize_subject_payload(&mut remote);
    merge_shared_sync_payloads(local, remote, device_id, exported_at)
}

pub fn count_payload_entities(payload: &SharedSyncPayload) -> i64 {
    [
        payload.subjects.len(),
        payload.study_modes.len(),
        payload.focus_sessions.len(),
        payload.app_events.len(),
        payload.checklist_tasks.len(),
        payload.today_plan_items.len(),
        payload.schedule_blocks.len(),
        payload.schedule_templates.len(),
        payload.daily_reviews.len(),
        payload.weekly_reviews.len(),
    ]
    .iter()
    .map(|value| *value as i64)
    .sum()
}

pub fn count_payload_deleted_entities(payload: &SharedSyncPayload) -> i64 {
    payload
        .subjects
        .iter()
        .filter(|item| item.deleted_at.is_some())
        .count() as i64
        + payload
            .study_modes
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .focus_sessions
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .app_events
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .checklist_tasks
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .today_plan_items
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .schedule_blocks
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .schedule_templates
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .daily_reviews
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .weekly_reviews
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
}

fn active_device_id(connection: &Connection) -> Result<Option<String>, String> {
    let active_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM study_modes WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_exists > 0 {
        ensure_device_id(connection).map(Some)
    } else {
        Ok(None)
    }
}

pub fn shared_active_study_snapshot(
    payload: &SharedSyncPayload,
) -> Option<SharedActiveStudySnapshot> {
    shared_active_study_snapshot_from_modes(&payload.study_modes)
}

fn shared_active_study_snapshot_from_modes(
    study_modes: &[SharedStudyMode],
) -> Option<SharedActiveStudySnapshot> {
    study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .fold(
            None,
            |best: Option<(&SharedStudyMode, (i64, i64))>, item| {
                let score = active_sort_key(item);
                match best {
                    Some((current, current_score)) if current_score >= score => {
                        Some((current, current_score))
                    }
                    _ => Some((item, score)),
                }
            },
        )
        .map(|(item, _)| SharedActiveStudySnapshot {
            sync_id: item.sync_id.clone(),
            status: item.status.clone(),
            phase: item.phase.clone(),
            subject_sync_id: item.subject_sync_id.clone(),
            current_session_sync_id: item.current_session_sync_id.clone(),
            phase_started_at: item.phase_started_at,
            paused_at: item.paused_at,
            round_number: item.round_number,
            current_break_type: item.current_break_type.clone(),
            ended_at: item.ended_at,
            state_revision: item.state_revision,
            updated_at: item.updated_at,
        })
}

fn canonicalize_subject_payload(payload: &mut SharedSyncPayload) {
    let mut aliases = HashMap::<String, String>::new();
    for legacy in [
        ("subject-politics", "subject-1"),
        ("subject-english", "subject-2"),
        ("subject-math", "subject-3"),
        ("subject-major", "subject-4"),
    ] {
        aliases.insert(legacy.0.to_string(), legacy.1.to_string());
    }

    for subject in &payload.subjects {
        let canonical =
            canonical_subject_sync_id_for_subject(&subject.sync_id, subject.name.as_deref());
        if canonical != subject.sync_id {
            aliases.insert(subject.sync_id.clone(), canonical);
        }
    }

    for subject in &mut payload.subjects {
        subject.sync_id = aliases
            .get(&subject.sync_id)
            .cloned()
            .unwrap_or_else(|| canonical_subject_sync_id(&subject.sync_id));
    }
    payload.subjects.retain(|subject| {
        !(is_default_subject_sync_id(&subject.sync_id) && subject.deleted_at.is_some())
    });
    for mode in &mut payload.study_modes {
        canonicalize_subject_option(&mut mode.subject_sync_id, &aliases);
    }
    for session in &mut payload.focus_sessions {
        canonicalize_subject_option(&mut session.subject_sync_id, &aliases);
    }
    for task in &mut payload.checklist_tasks {
        canonicalize_subject_option(&mut task.subject_sync_id, &aliases);
    }
    for item in &mut payload.today_plan_items {
        canonicalize_subject_option(&mut item.subject_sync_id, &aliases);
    }
    for block in &mut payload.schedule_blocks {
        canonicalize_subject_option(&mut block.subject_sync_id, &aliases);
    }
    for template in &mut payload.schedule_templates {
        canonicalize_subject_option(&mut template.subject_sync_id, &aliases);
    }
}

fn normalize_import_active_sessions(payload: &mut SharedSyncPayload) {
    let active_mode_ids = payload
        .study_modes
        .iter()
        .filter(|mode| mode.deleted_at.is_none())
        .filter(|mode| {
            mode.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .map(|mode| mode.sync_id.clone())
        .collect::<HashSet<_>>();
    let active_session_ids = payload
        .study_modes
        .iter()
        .filter_map(|mode| mode.current_session_sync_id.as_deref())
        .map(str::to_string)
        .collect::<HashSet<_>>();

    for session in &mut payload.focus_sessions {
        if session.deleted_at.is_some() || session.status.as_deref() != Some("running") {
            continue;
        }
        let belongs_to_active_mode = session
            .study_mode_sync_id
            .as_deref()
            .map(|mode_id| active_mode_ids.contains(mode_id))
            .unwrap_or(false);
        if active_session_ids.contains(&session.sync_id) || belongs_to_active_mode {
            continue;
        }

        session.status = Some("finished".to_string());
        session.ended_at = session.ended_at.or(Some(session.updated_at));
        session.end_reason = session
            .end_reason
            .clone()
            .or_else(|| Some("sync_takeover".to_string()));
    }
}

fn canonicalize_subject_option(value: &mut Option<String>, aliases: &HashMap<String, String>) {
    if let Some(sync_id) = value.as_deref() {
        *value = Some(
            aliases
                .get(sync_id)
                .cloned()
                .unwrap_or_else(|| canonical_subject_sync_id(sync_id)),
        );
    }
}

fn canonical_subject_sync_id_for_subject(sync_id: &str, name: Option<&str>) -> String {
    if let Some(canonical) = name.and_then(canonical_subject_sync_id_for_name) {
        return canonical.to_string();
    }
    canonical_subject_sync_id(sync_id)
}

fn canonical_subject_sync_id(sync_id: &str) -> String {
    match sync_id.trim() {
        "subject-politics" => "subject-1".to_string(),
        "subject-english" => "subject-2".to_string(),
        "subject-math" => "subject-3".to_string(),
        "subject-major" => "subject-4".to_string(),
        value => value.to_string(),
    }
}

fn canonical_subject_sync_id_for_name(name: &str) -> Option<&'static str> {
    match normalize_name(name).as_str() {
        "政治" => Some(DEFAULT_SUBJECT_SYNC_IDS[0]),
        "英语" => Some(DEFAULT_SUBJECT_SYNC_IDS[1]),
        "数学" => Some(DEFAULT_SUBJECT_SYNC_IDS[2]),
        "专业课" => Some(DEFAULT_SUBJECT_SYNC_IDS[3]),
        _ => None,
    }
}

fn is_default_subject_sync_id(sync_id: &str) -> bool {
    DEFAULT_SUBJECT_SYNC_IDS.contains(&sync_id)
}

pub fn import_shared_sync_payload(
    connection: &mut Connection,
    payload: &SharedSyncPayload,
) -> Result<(), String> {
    let mut payload = payload.clone();
    canonicalize_subject_payload(&mut payload);
    normalize_import_active_sessions(&mut payload);
    {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        eprintln!("shared payload import: subjects {}", payload.subjects.len());
        import_subjects(&transaction, &payload.subjects)?;
        eprintln!(
            "shared payload import: checklist_tasks {}",
            payload.checklist_tasks.len()
        );
        import_checklist_tasks(&transaction, &payload.checklist_tasks)?;
        eprintln!(
            "shared payload import: today_plan_items {}",
            payload.today_plan_items.len()
        );
        import_today_plan_items(&transaction, &payload.today_plan_items)?;
        eprintln!(
            "shared payload import: schedule_templates {}",
            payload.schedule_templates.len()
        );
        import_schedule_templates(&transaction, &payload.schedule_templates)?;
        eprintln!(
            "shared payload import: schedule_blocks {}",
            payload.schedule_blocks.len()
        );
        import_schedule_blocks(&transaction, &payload.schedule_blocks)?;
        eprintln!(
            "shared payload import: daily_reviews {}",
            payload.daily_reviews.len()
        );
        import_daily_reviews(&transaction, &payload.daily_reviews)?;
        eprintln!(
            "shared payload import: weekly_reviews {}",
            payload.weekly_reviews.len()
        );
        import_weekly_reviews(&transaction, &payload.weekly_reviews)?;
        eprintln!("shared payload import: high priority commit");
        transaction.commit().map_err(|error| error.to_string())?;
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    eprintln!(
        "shared payload import: focus_sessions {}",
        payload.focus_sessions.len()
    );
    import_focus_sessions(&transaction, &payload.focus_sessions)?;
    eprintln!(
        "shared payload import: study_modes {}",
        payload.study_modes.len()
    );
    import_study_modes(&transaction, &payload.study_modes)?;
    eprintln!(
        "shared payload import: schedule_blocks second pass {}",
        payload.schedule_blocks.len()
    );
    import_schedule_blocks(&transaction, &payload.schedule_blocks)?;
    eprintln!(
        "shared payload import: app_events {}",
        payload.app_events.len()
    );
    import_app_events(&transaction, &payload.app_events)?;
    eprintln!("shared payload import: active_conflicts");
    resolve_local_active_conflicts(&transaction)?;
    eprintln!("shared payload import: commit");
    transaction.commit().map_err(|error| error.to_string())
}

pub fn mark_entity_deleted(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    deleted_at: i64,
) -> Result<(), String> {
    let sync_id = resolve_or_create_sync_id(connection, entity_type, local_id, None, deleted_at)?;
    upsert_sync_meta(
        connection,
        entity_type,
        local_id,
        &sync_id,
        deleted_at,
        Some(deleted_at),
    )?;
    Ok(())
}

fn merge_latest_by_sync_id<T, Id, Updated, Deleted>(
    local: &[T],
    remote: &[T],
    id_of: Id,
    updated_at_of: Updated,
    deleted_at_of: Deleted,
) -> Vec<T>
where
    T: Clone,
    Id: Fn(&T) -> &str,
    Updated: Fn(&T) -> i64,
    Deleted: Fn(&T) -> Option<i64>,
{
    let mut merged: Vec<T> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = id_of(item).trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            let current = &merged[position];
            let item_updated_at = updated_at_of(item);
            let current_updated_at = updated_at_of(current);
            let tombstone_tie_wins = item_updated_at == current_updated_at
                && deleted_at_of(item).is_some()
                && deleted_at_of(current).is_none();
            if item_updated_at > current_updated_at || tombstone_tie_wins {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }

    merged
}

fn merge_study_modes(
    local: &[SharedStudyMode],
    remote: &[SharedStudyMode],
) -> Vec<SharedStudyMode> {
    let mut merged: Vec<SharedStudyMode> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = item.sync_id.trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            if should_replace_study_mode(&merged[position], item) {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }
    merged
}

fn should_replace_study_mode(existing: &SharedStudyMode, candidate: &SharedStudyMode) -> bool {
    let existing_deleted = existing.deleted_at.is_some();
    let candidate_deleted = candidate.deleted_at.is_some();
    if candidate.updated_at == existing.updated_at && candidate_deleted && !existing_deleted {
        return true;
    }

    let existing_revision = existing.state_revision.unwrap_or(0).max(0);
    let candidate_revision = candidate.state_revision.unwrap_or(0).max(0);
    if candidate_revision != existing_revision {
        return candidate_revision > existing_revision;
    }

    if existing_deleted != candidate_deleted {
        return candidate_deleted && candidate.updated_at >= existing.updated_at;
    }

    candidate.updated_at > existing.updated_at
}

fn merge_focus_sessions(
    local: &[SharedFocusSession],
    remote: &[SharedFocusSession],
) -> Vec<SharedFocusSession> {
    let mut merged: Vec<SharedFocusSession> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = item.sync_id.trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            if should_replace_focus_session(&merged[position], item) {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }
    merged
}

fn should_replace_focus_session(
    existing: &SharedFocusSession,
    candidate: &SharedFocusSession,
) -> bool {
    let existing_deleted = existing.deleted_at.is_some();
    let candidate_deleted = candidate.deleted_at.is_some();
    if candidate.updated_at == existing.updated_at && candidate_deleted && !existing_deleted {
        return true;
    }

    let existing_running = existing.status.as_deref() == Some("running");
    let candidate_running = candidate.status.as_deref() == Some("running");
    if existing_running != candidate_running {
        return !candidate_running && candidate.ended_at.is_some();
    }

    if existing_deleted != candidate_deleted {
        return candidate_deleted && candidate.updated_at >= existing.updated_at;
    }

    candidate.updated_at > existing.updated_at
}

fn active_sort_key(mode: &SharedStudyMode) -> (i64, i64) {
    (
        mode.state_revision.unwrap_or(0).max(0),
        mode.updated_at.max(0),
    )
}

#[derive(Debug, Clone, Copy)]
struct ActiveLogicalPosition {
    round: i64,
    phase_rank: i64,
    progress_seconds: i64,
}

fn should_keep_local_active(
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    remote_active: Option<&SharedActiveStudySnapshot>,
    now_millis: i64,
) -> bool {
    let (Some(local_active), Some(remote_active)) = (local_active, remote_active) else {
        return false;
    };

    let local_revision = local_active.state_revision.unwrap_or(0).max(0);
    let remote_revision = remote_active.state_revision.unwrap_or(0).max(0);
    if local_revision != remote_revision {
        return local_revision > remote_revision;
    }

    if local_active.updated_at >= remote_active.updated_at {
        return true;
    }

    if local_active.sync_id != remote_active.sync_id {
        return false;
    }

    let local_mode = local
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id);
    let remote_mode = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == remote_active.sync_id);

    match (local_mode, remote_mode) {
        (Some(local_mode), Some(remote_mode)) => {
            active_mode_regresses(local_mode, remote_mode, now_millis)
        }
        _ => false,
    }
}

fn classify_active_remote_merge(
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    now_millis: i64,
) -> ActiveRemoteMergeDecision {
    let Some(local_active) = local_active else {
        return ActiveRemoteMergeDecision::Default;
    };
    let Some(local_mode) = local
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
    else {
        return ActiveRemoteMergeDecision::Default;
    };
    let Some(remote_mode) = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
    else {
        if shared_active_study_snapshot(remote).is_some() {
            return ActiveRemoteMergeDecision::KeepLocalActive;
        }
        return ActiveRemoteMergeDecision::Default;
    };

    if remote_mode.updated_at <= local_mode.updated_at {
        return if active_mode_regresses(local_mode, remote_mode, now_millis) {
            ActiveRemoteMergeDecision::KeepLocalActive
        } else {
            ActiveRemoteMergeDecision::Default
        };
    }

    if is_remote_active_command(local_mode, remote_mode) {
        return ActiveRemoteMergeDecision::AcceptRemoteCommand;
    }

    if active_mode_regresses(local_mode, remote_mode, now_millis) {
        return ActiveRemoteMergeDecision::KeepLocalActive;
    }

    ActiveRemoteMergeDecision::Default
}

fn is_remote_active_command(local: &SharedStudyMode, remote: &SharedStudyMode) -> bool {
    let remote_status = remote.status.as_deref().unwrap_or_default();
    let remote_phase = remote.phase.as_deref().unwrap_or_default();
    if matches!(remote_status, "finished" | "emergency_exited")
        || matches!(remote_phase, "finished" | "emergency_exited")
        || remote.ended_at.is_some()
    {
        return true;
    }

    let local_phase = local.phase.as_deref().unwrap_or_default();
    let local_paused = local_phase == "paused" || local.paused_at.is_some();
    let remote_paused = remote_phase == "paused" || remote.paused_at.is_some();
    if remote_paused && !local_paused {
        return true;
    }
    if local_paused && !remote_paused && matches!(remote_phase, "focus" | "awaiting_break") {
        return true;
    }
    local_phase == "awaiting_break" && remote_phase == "break"
}

fn active_mode_regresses(
    local: &SharedStudyMode,
    remote: &SharedStudyMode,
    now_millis: i64,
) -> bool {
    let local_position = active_logical_position(local, now_millis);
    let remote_position = active_logical_position(remote, now_millis);

    if remote_position.round != local_position.round {
        return remote_position.round < local_position.round;
    }
    if remote_position.phase_rank != local_position.phase_rank {
        return remote_position.phase_rank < local_position.phase_rank;
    }

    remote_position.progress_seconds + 5 < local_position.progress_seconds
}

fn active_logical_position(mode: &SharedStudyMode, now_millis: i64) -> ActiveLogicalPosition {
    let round = mode.round_number.unwrap_or(1).max(1);
    let phase = effective_shared_phase(mode);
    let phase_rank = match phase.as_str() {
        "awaiting_break" => 1,
        "break" => 2,
        "finished" | "emergency_exited" => 3,
        _ => 0,
    };
    let accumulated = mode.accumulated_study_seconds.unwrap_or(0).max(0);
    let planned = mode.planned_seconds.unwrap_or(i64::MAX / 4).max(0);
    let remaining = planned.saturating_sub(accumulated);
    let current_focus_seconds = if phase == "focus" {
        let focus_seconds = mode.focus_seconds.unwrap_or(remaining).max(0);
        phase_elapsed_seconds(mode, now_millis)
            .min(focus_seconds)
            .min(remaining)
            .max(0)
    } else {
        0
    };

    ActiveLogicalPosition {
        round,
        phase_rank,
        progress_seconds: (accumulated + current_focus_seconds).min(planned).max(0),
    }
}

fn effective_shared_phase(mode: &SharedStudyMode) -> String {
    let phase = mode.phase.as_deref().unwrap_or("focus");
    if phase == "paused" {
        mode.paused_from_phase
            .as_deref()
            .filter(|value| !value.trim().is_empty() && *value != "paused")
            .unwrap_or("focus")
            .to_string()
    } else {
        phase.to_string()
    }
}

fn phase_elapsed_seconds(mode: &SharedStudyMode, now_millis: i64) -> i64 {
    if mode.phase.as_deref() == Some("paused") || mode.paused_at.is_some() {
        return mode
            .paused_stage_elapsed_seconds
            .or(mode.phase_paused_seconds)
            .unwrap_or(0)
            .max(0);
    }

    mode.phase_started_at
        .map(|started_at| ((now_millis - started_at).max(0)) / 1000)
        .unwrap_or(0)
}

fn restore_active_from_payload(
    study_modes: &mut Vec<SharedStudyMode>,
    focus_sessions: &mut Vec<SharedFocusSession>,
    source: &SharedSyncPayload,
    snapshot: &SharedActiveStudySnapshot,
) {
    let Some(source_mode) = source
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == snapshot.sync_id)
        .cloned()
    else {
        return;
    };

    if let Some(position) = study_modes
        .iter()
        .position(|mode| mode.sync_id == source_mode.sync_id)
    {
        study_modes[position] = source_mode.clone();
    } else {
        study_modes.push(source_mode.clone());
    }

    if let Some(session_sync_id) = source_mode.current_session_sync_id.as_deref() {
        if let Some(source_session) = source
            .focus_sessions
            .iter()
            .find(|session| session.sync_id == session_sync_id)
            .cloned()
        {
            if let Some(position) = focus_sessions
                .iter()
                .position(|session| session.sync_id == source_session.sync_id)
            {
                focus_sessions[position] = source_session;
            } else {
                focus_sessions.push(source_session);
            }
        }
    }
}

fn restore_matching_active_from_remote(
    study_modes: &mut Vec<SharedStudyMode>,
    focus_sessions: &mut Vec<SharedFocusSession>,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
) {
    let Some(local_active) = local_active else {
        return;
    };
    let Some(remote_mode) = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
        .cloned()
    else {
        return;
    };

    if let Some(position) = study_modes
        .iter()
        .position(|mode| mode.sync_id == remote_mode.sync_id)
    {
        study_modes[position] = remote_mode.clone();
    } else {
        study_modes.push(remote_mode.clone());
    }

    if let Some(session_sync_id) = remote_mode.current_session_sync_id.as_deref() {
        if let Some(remote_session) = remote
            .focus_sessions
            .iter()
            .find(|session| session.sync_id == session_sync_id)
            .cloned()
        {
            if let Some(position) = focus_sessions
                .iter()
                .position(|session| session.sync_id == remote_session.sync_id)
            {
                focus_sessions[position] = remote_session;
            } else {
                focus_sessions.push(remote_session);
            }
        }
    }
}

fn resolve_shared_active_conflicts(
    study_modes: &mut [SharedStudyMode],
    focus_sessions: &mut [SharedFocusSession],
    resolved_at: i64,
    preferred_winner_sync_id: Option<&str>,
) {
    let preferred_winner_sync_id = preferred_winner_sync_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let winner_sync_id = preferred_winner_sync_id
        .and_then(|preferred| {
            study_modes
                .iter()
                .find(|item| {
                    item.deleted_at.is_none()
                        && item.sync_id == preferred
                        && item
                            .status
                            .as_deref()
                            .map(is_shared_running_status)
                            .unwrap_or(false)
                })
                .map(|item| item.sync_id.clone())
        })
        .or_else(|| {
            study_modes
                .iter()
                .filter(|item| item.deleted_at.is_none())
                .filter(|item| {
                    item.status
                        .as_deref()
                        .map(is_shared_running_status)
                        .unwrap_or(false)
                })
                .max_by_key(|item| active_sort_key(item))
                .map(|item| item.sync_id.clone())
        });

    let Some(winner_sync_id) = winner_sync_id else {
        return;
    };

    let mut losing_mode_ids = HashSet::new();
    let mut losing_session_ids = HashSet::new();

    for mode in study_modes.iter_mut() {
        let is_running = mode
            .status
            .as_deref()
            .map(is_shared_running_status)
            .unwrap_or(false);
        if !is_running || mode.sync_id == winner_sync_id {
            continue;
        }

        losing_mode_ids.insert(mode.sync_id.clone());
        if let Some(session_sync_id) = mode.current_session_sync_id.as_deref() {
            losing_session_ids.insert(session_sync_id.to_string());
        }

        mode.status = Some("finished".to_string());
        mode.phase = Some("finished".to_string());
        mode.ended_at = mode.ended_at.or(Some(resolved_at));
        mode.current_session_sync_id = None;
        mode.finish_reason = Some("sync_takeover".to_string());
        mode.updated_at = mode.updated_at.max(resolved_at);
    }

    if losing_mode_ids.is_empty() && losing_session_ids.is_empty() {
        return;
    }

    for session in focus_sessions.iter_mut() {
        let belongs_to_losing_mode = session
            .study_mode_sync_id
            .as_deref()
            .map(|sync_id| losing_mode_ids.contains(sync_id))
            .unwrap_or(false);
        let is_losing_current_session = losing_session_ids.contains(&session.sync_id);
        let is_running = session.status.as_deref() == Some("running");

        if is_running && (belongs_to_losing_mode || is_losing_current_session) {
            session.status = Some("finished".to_string());
            session.ended_at = session.ended_at.or(Some(resolved_at));
            session.end_reason = Some("sync_takeover".to_string());
            session.updated_at = session.updated_at.max(resolved_at);
        }
    }
}

fn is_shared_running_status(status: &str) -> bool {
    status == "running" || status == "active"
}

fn to_shared_study_status(status: &str) -> &str {
    if status == "active" {
        "running"
    } else {
        status
    }
}

fn to_desktop_study_status(status: &str) -> &str {
    if status == "running" {
        "active"
    } else {
        status
    }
}

fn break_type_after_round(round_number: i64, long_break_interval: i64) -> String {
    if long_break_interval > 0 && round_number > 0 && round_number % long_break_interval == 0 {
        "long".to_string()
    } else {
        "short".to_string()
    }
}

fn export_subjects(connection: &Connection) -> Result<Vec<SharedSubject>, String> {
    let rows = load_subject_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        if !row.enabled {
            continue;
        }
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let preferred_sync_id = default_subject_sync_id(&row.name, row.id);
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_SUBJECT,
            row.id,
            Some(preferred_sync_id),
            updated_at,
        )?;
        let meta = get_sync_meta_by_sync_id(connection, &sync_id)?;
        if meta.as_ref().and_then(|item| item.deleted_at).is_some() {
            continue;
        }

        payload.push(SharedSubject {
            sync_id,
            name: Some(row.name),
            color: row.color,
            enabled: Some(row.enabled),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SUBJECT)?);
    Ok(payload)
}

fn export_study_modes(connection: &Connection) -> Result<Vec<SharedStudyMode>, String> {
    let rows = load_study_mode_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let started_at_millis = parse_rfc3339_millis(&row.started_at)?;
        let phase_started_at_millis = parse_rfc3339_millis(&row.phase_started_at)?;
        let paused_at_millis = row
            .paused_at
            .as_deref()
            .map(parse_rfc3339_millis)
            .transpose()?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_STUDY_MODE, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedStudyMode {
            sync_id,
            state_revision: Some(row.state_revision.max(1)),
            mode: Some(row.mode),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            planned_seconds: Some(row.planned_seconds),
            focus_seconds: Some(row.focus_seconds),
            break_seconds: Some(row.break_seconds),
            long_break_seconds: Some(row.long_break_seconds),
            long_break_interval: Some(row.long_break_interval),
            phase: Some(if row.paused_at.is_some() {
                "paused".to_string()
            } else {
                row.phase.clone()
            }),
            round_number: Some(row.cycle_index),
            started_at: Some(started_at_millis),
            phase_started_at: Some(phase_started_at_millis),
            paused_at: paused_at_millis,
            paused_from_phase: row.paused_at.as_ref().map(|_| row.phase.clone()),
            accumulated_study_seconds: Some(row.accumulated_study_seconds),
            total_paused_seconds: Some(row.total_paused_seconds),
            phase_paused_seconds: Some(row.paused_stage_elapsed_seconds),
            paused_stage_elapsed_seconds: Some(row.paused_stage_elapsed_seconds),
            current_break_type: Some(break_type_after_round(
                row.cycle_index,
                row.long_break_interval,
            )),
            ended_at: row
                .ended_at
                .as_deref()
                .map(parse_rfc3339_millis)
                .transpose()?,
            current_session_sync_id: row.current_session_id.and_then(|session_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_FOCUS_SESSION,
                    Some(session_id),
                )
                .ok()
                .flatten()
            }),
            schedule_block_sync_id: row.schedule_block_id.and_then(|block_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_BLOCK,
                    Some(block_id),
                )
                .ok()
                .flatten()
            }),
            today_plan_item_sync_id: row.today_plan_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            status: Some(to_shared_study_status(&row.status).to_string()),
            finish_reason: row.finish_reason,
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_STUDY_MODE)?);
    Ok(payload)
}

fn export_focus_sessions(connection: &Connection) -> Result<Vec<SharedFocusSession>, String> {
    let rows = load_focus_session_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_FOCUS_SESSION, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedFocusSession {
            sync_id,
            study_mode_sync_id: resolve_study_mode_sync_id_for_session(connection, row.id)?,
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            mode: Some(row.mode),
            planned_seconds: Some(row.planned_seconds),
            actual_seconds: Some(row.actual_seconds),
            started_at: Some(parse_rfc3339_millis(&row.started_at)?),
            ended_at: row
                .ended_at
                .as_deref()
                .map(parse_rfc3339_millis)
                .transpose()?,
            status: Some(row.status),
            end_reason: row.end_reason,
            interruption_count: Some(row.interruption_count),
            emergency_exit_count: Some(row.emergency_exit_count),
            paused_seconds: Some(row.paused_seconds),
            followed_by_break_type: row.followed_by_break_type,
            schedule_block_sync_id: row.schedule_block_id.and_then(|block_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_BLOCK,
                    Some(block_id),
                )
                .ok()
                .flatten()
            }),
            today_plan_item_sync_id: row.today_plan_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_FOCUS_SESSION)?);
    Ok(payload)
}

fn export_app_events(connection: &Connection) -> Result<Vec<SharedAppEvent>, String> {
    let rows = load_app_event_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let created_at = parse_rfc3339_millis(&row.created_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_APP_EVENT, row.id, None, created_at)?;
        payload.push(SharedAppEvent {
            sync_id,
            study_mode_sync_id: None,
            focus_session_sync_id: resolve_existing_sync_id_by_local_id(
                connection,
                ENTITY_FOCUS_SESSION,
                Some(row.session_id),
            )?,
            package_name: Some(row.process_name),
            app_name: row.window_title.clone(),
            event_type: Some(row.event_type),
            action: row.action_taken.clone(),
            created_at: Some(created_at),
            updated_at: created_at,
            deleted_at: None,
        });
    }
    Ok(payload)
}

fn export_checklist_tasks(connection: &Connection) -> Result<Vec<SharedChecklistTask>, String> {
    let rows = load_checklist_task_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_CHECKLIST_TASK, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedChecklistTask {
            sync_id,
            category_key: Some(map_board_scope_to_category_key(&row.board_scope)),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            title: Some(row.title),
            note: row.note,
            due_date: row.due_date,
            sort_order: Some(row.sort_order as f64),
            completed: Some(row.completed),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_CHECKLIST_TASK)?);
    Ok(payload)
}

fn export_today_plan_items(connection: &Connection) -> Result<Vec<SharedTodayPlanItem>, String> {
    let rows = load_today_plan_item_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            row.id,
            None,
            updated_at,
        )?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedTodayPlanItem {
            sync_id,
            today_date: Some(row.today_date),
            source_task_sync_id: row.source_task_id.and_then(|source_task_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_CHECKLIST_TASK,
                    Some(source_task_id),
                )
                .ok()
                .flatten()
            }),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            title: Some(row.title),
            note: row.note,
            due_date: row.due_date,
            sort_order: Some(row.sort_order as f64),
            completed: Some(row.completed),
            synced_source_completion: Some(row.synced_source_completion),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_TODAY_PLAN_ITEM)?);
    Ok(payload)
}

fn export_schedule_blocks(connection: &Connection) -> Result<Vec<SharedScheduleBlock>, String> {
    let rows = load_schedule_block_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_SCHEDULE_BLOCK, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedScheduleBlock {
            sync_id,
            schedule_date: Some(row.schedule_date),
            title: Some(row.title),
            note: row.note,
            category_key: Some(row.category_key),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            source_today_item_sync_id: row.source_today_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            template_sync_id: row.template_id.and_then(|template_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_TEMPLATE,
                    Some(template_id),
                )
                .ok()
                .flatten()
            }),
            start_minute: Some(row.start_minute),
            end_minute: Some(row.end_minute),
            status: Some(row.status),
            linked_study_mode_sync_id: row.linked_study_mode_id.and_then(|study_mode_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_STUDY_MODE,
                    Some(study_mode_id),
                )
                .ok()
                .flatten()
            }),
            linked_focus_session_sync_id: row.linked_focus_session_id.and_then(|session_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_FOCUS_SESSION,
                    Some(session_id),
                )
                .ok()
                .flatten()
            }),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SCHEDULE_BLOCK)?);
    Ok(payload)
}

fn export_schedule_templates(
    connection: &Connection,
) -> Result<Vec<SharedScheduleTemplate>, String> {
    let rows = load_schedule_template_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            row.id,
            None,
            updated_at,
        )?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedScheduleTemplate {
            sync_id,
            title: Some(row.title),
            note: row.note,
            category_key: Some(row.category_key),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            weekdays: Some(parse_weekdays_json(&row.weekdays)),
            start_minute: Some(row.start_minute),
            end_minute: Some(row.end_minute),
            enabled: Some(row.enabled),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SCHEDULE_TEMPLATE)?);
    Ok(payload)
}

fn export_daily_reviews(connection: &Connection) -> Result<Vec<SharedDailyReview>, String> {
    let rows = load_daily_review_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_DAILY_REVIEW, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedDailyReview {
            sync_id,
            review_date: Some(row.review_date),
            summary: row.summary,
            blockers: row.blockers,
            tomorrow_focus: row.tomorrow_focus,
            mood_score: Some(row.mood_score),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_DAILY_REVIEW)?);
    Ok(payload)
}

fn export_weekly_reviews(connection: &Connection) -> Result<Vec<SharedWeeklyReview>, String> {
    let rows = load_weekly_review_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_WEEKLY_REVIEW, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedWeeklyReview {
            sync_id,
            week_start_date: Some(row.week_start_date),
            summary: row.summary,
            blockers: row.blockers,
            next_week_focus: row.next_week_focus,
            mood_score: Some(row.mood_score),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_WEEKLY_REVIEW)?);
    Ok(payload)
}

fn export_tombstones<T>(connection: &Connection, entity_type: &str) -> Result<Vec<T>, String>
where
    T: From<DeletedPayload>,
{
    let mut statement = connection
        .prepare(
            "
            SELECT sync_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND deleted_at IS NOT NULL
            ORDER BY deleted_at ASC, local_id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![entity_type], |row| {
            Ok(DeletedPayload {
                sync_id: row.get(0)?,
                deleted_at: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|item| item.map(Into::into).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
}

#[derive(Debug, Clone)]
struct DeletedPayload {
    sync_id: String,
    deleted_at: Option<i64>,
}

impl From<DeletedPayload> for SharedSubject {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            name: None,
            color: None,
            enabled: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedStudyMode {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            state_revision: None,
            mode: None,
            subject_sync_id: None,
            planned_seconds: None,
            focus_seconds: None,
            break_seconds: None,
            long_break_seconds: None,
            long_break_interval: None,
            phase: None,
            round_number: None,
            started_at: None,
            phase_started_at: None,
            paused_at: None,
            paused_from_phase: None,
            accumulated_study_seconds: None,
            total_paused_seconds: None,
            phase_paused_seconds: None,
            paused_stage_elapsed_seconds: None,
            current_break_type: None,
            ended_at: None,
            current_session_sync_id: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            status: None,
            finish_reason: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedFocusSession {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            study_mode_sync_id: None,
            subject_sync_id: None,
            mode: None,
            planned_seconds: None,
            actual_seconds: None,
            started_at: None,
            ended_at: None,
            status: None,
            end_reason: None,
            interruption_count: None,
            emergency_exit_count: None,
            paused_seconds: None,
            followed_by_break_type: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedAppEvent {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            study_mode_sync_id: None,
            focus_session_sync_id: None,
            package_name: None,
            app_name: None,
            event_type: None,
            action: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedChecklistTask {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedTodayPlanItem {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            today_date: None,
            source_task_sync_id: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            synced_source_completion: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedScheduleBlock {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            schedule_date: None,
            title: None,
            note: None,
            category_key: None,
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: None,
            start_minute: None,
            end_minute: None,
            status: None,
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedScheduleTemplate {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            title: None,
            note: None,
            category_key: None,
            subject_sync_id: None,
            weekdays: None,
            start_minute: None,
            end_minute: None,
            enabled: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedDailyReview {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            review_date: None,
            summary: None,
            blockers: None,
            tomorrow_focus: None,
            mood_score: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedWeeklyReview {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            week_start_date: None,
            summary: None,
            blockers: None,
            next_week_focus: None,
            mood_score: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

fn import_subjects(connection: &Connection, items: &[SharedSubject]) -> Result<(), String> {
    for item in items {
        let sync_id = canonical_subject_sync_id_for_subject(&item.sync_id, item.name.as_deref());
        if let Some(deleted_at) = item.deleted_at {
            if is_default_subject_sync_id(&sync_id) {
                continue;
            }
            delete_local_row_by_sync_id(connection, ENTITY_SUBJECT, &sync_id, deleted_at)?;
            continue;
        }

        let Some(name) = item
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let enabled = item.enabled.unwrap_or(true);
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_subject_row(
            connection,
            &sync_id,
            name,
            item.color.clone(),
            enabled,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_study_modes(connection: &Connection, items: &[SharedStudyMode]) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(connection, ENTITY_STUDY_MODE, &item.sync_id, deleted_at)?;
            continue;
        }

        let Some(mode) = item
            .mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(phase) = item
            .phase
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(status) = item
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let current_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.current_session_sync_id.as_deref(),
        )?;
        let schedule_block_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            item.schedule_block_sync_id.as_deref(),
        )?;
        let today_plan_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.today_plan_item_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);
        let desktop_status = to_desktop_study_status(status);
        let desktop_phase = if phase == "paused" {
            item.paused_from_phase
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "paused")
                .unwrap_or("focus")
        } else {
            phase
        };
        let paused_at = if phase == "paused" || item.paused_at.is_some() {
            Some(millis_to_rfc3339(item.paused_at.unwrap_or(item.updated_at)))
        } else {
            None
        };

        upsert_study_mode_row(
            connection,
            &item.sync_id,
            item.state_revision.unwrap_or(1).max(1),
            mode,
            subject_id,
            item.planned_seconds.unwrap_or(0),
            item.focus_seconds.unwrap_or(0),
            item.break_seconds.unwrap_or(0),
            item.long_break_seconds.unwrap_or(900),
            item.long_break_interval.unwrap_or(4),
            desktop_phase,
            item.round_number.unwrap_or(1),
            &millis_to_rfc3339(item.started_at.unwrap_or(item.updated_at)),
            &millis_to_rfc3339(item.phase_started_at.unwrap_or(item.updated_at)),
            paused_at,
            item.accumulated_study_seconds.unwrap_or(0),
            item.total_paused_seconds.unwrap_or(0),
            item.paused_stage_elapsed_seconds
                .or(item.phase_paused_seconds)
                .unwrap_or(0),
            item.ended_at
                .as_ref()
                .map(|value| millis_to_rfc3339(*value)),
            current_session_id,
            schedule_block_id,
            today_plan_item_id,
            desktop_status,
            item.finish_reason.clone(),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_focus_sessions(
    connection: &Connection,
    items: &[SharedFocusSession],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_FOCUS_SESSION,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(mode) = item
            .mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(status) = item
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let schedule_block_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            item.schedule_block_sync_id.as_deref(),
        )?;
        let today_plan_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.today_plan_item_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_focus_session_row(
            connection,
            &item.sync_id,
            mode,
            subject_id,
            item.planned_seconds.unwrap_or(0),
            item.actual_seconds.unwrap_or(0),
            &millis_to_rfc3339(item.started_at.unwrap_or(item.updated_at)),
            item.ended_at
                .as_ref()
                .map(|value| millis_to_rfc3339(*value)),
            status,
            item.end_reason.clone(),
            item.interruption_count.unwrap_or(0),
            item.emergency_exit_count.unwrap_or(0),
            item.paused_seconds.unwrap_or(0),
            item.followed_by_break_type.clone(),
            schedule_block_id,
            today_plan_item_id,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_app_events(connection: &Connection, items: &[SharedAppEvent]) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(connection, ENTITY_APP_EVENT, &item.sync_id, deleted_at)?;
            continue;
        }

        let Some(package_name) = item
            .package_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(event_type) = item
            .event_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let focus_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.focus_session_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));

        upsert_app_event_row(
            connection,
            &item.sync_id,
            focus_session_id,
            package_name,
            item.app_name.clone(),
            event_type,
            item.action.clone(),
            &created_at,
        )?;
    }

    Ok(())
}

fn import_checklist_tasks(
    connection: &Connection,
    items: &[SharedChecklistTask],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_CHECKLIST_TASK,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(category_key) = item
            .category_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let board_scope = board_scope_for_category_key(category_key);
        ensure_checklist_column(&connection, &board_scope)?;
        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let column_id = get_first_checklist_column_id(&connection, &board_scope)?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_checklist_task_row(
            connection,
            &item.sync_id,
            &board_scope,
            subject_id,
            column_id,
            title,
            item.note.clone(),
            item.due_date.clone(),
            item.sort_order
                .map(|value| value.round() as i64)
                .unwrap_or(0),
            item.completed.unwrap_or(false),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_today_plan_items(
    connection: &Connection,
    items: &[SharedTodayPlanItem],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_TODAY_PLAN_ITEM,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(today_date) = item
            .today_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let source_task_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_CHECKLIST_TASK,
            item.source_task_sync_id.as_deref(),
        )?;
        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_today_plan_item_row(
            connection,
            &item.sync_id,
            today_date,
            source_task_id,
            subject_id,
            title,
            item.note.clone(),
            item.due_date.clone(),
            item.sort_order
                .map(|value| value.round() as i64)
                .unwrap_or(0),
            item.completed.unwrap_or(false),
            item.synced_source_completion.unwrap_or(false),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_schedule_templates(
    connection: &Connection,
    items: &[SharedScheduleTemplate],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_SCHEDULE_TEMPLATE,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);
        let weekdays = serde_json::to_string(&item.weekdays.clone().unwrap_or_default())
            .map_err(|error| error.to_string())?;

        upsert_schedule_template_row(
            connection,
            &item.sync_id,
            title,
            item.note.clone(),
            item.category_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("general"),
            subject_id,
            &weekdays,
            item.start_minute.unwrap_or(360),
            item.end_minute.unwrap_or(420),
            item.enabled.unwrap_or(true),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_schedule_blocks(
    connection: &Connection,
    items: &[SharedScheduleBlock],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(schedule_date) = item
            .schedule_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let source_today_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.source_today_item_sync_id.as_deref(),
        )?;
        let template_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            item.template_sync_id.as_deref(),
        )?;
        let linked_study_mode_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_STUDY_MODE,
            item.linked_study_mode_sync_id.as_deref(),
        )?;
        let linked_focus_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.linked_focus_session_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_schedule_block_row(
            connection,
            &item.sync_id,
            schedule_date,
            title,
            item.note.clone(),
            item.category_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("general"),
            subject_id,
            source_today_item_id,
            template_id,
            item.start_minute.unwrap_or(360),
            item.end_minute.unwrap_or(420),
            item.status
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("planned"),
            linked_study_mode_id,
            linked_focus_session_id,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_daily_reviews(
    connection: &Connection,
    items: &[SharedDailyReview],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_DAILY_REVIEW,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(review_date) = item
            .review_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_daily_review_row(
            connection,
            &item.sync_id,
            review_date,
            item.summary.clone(),
            item.blockers.clone(),
            item.tomorrow_focus.clone(),
            item.mood_score.unwrap_or(3).clamp(1, 5),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_weekly_reviews(
    connection: &Connection,
    items: &[SharedWeeklyReview],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_WEEKLY_REVIEW,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(week_start_date) = item
            .week_start_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_weekly_review_row(
            connection,
            &item.sync_id,
            week_start_date,
            item.summary.clone(),
            item.blockers.clone(),
            item.next_week_focus.clone(),
            item.mood_score.unwrap_or(3).clamp(1, 5),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn load_subject_rows(connection: &Connection) -> Result<Vec<DesktopSubjectRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, name, color, enabled, created_at, updated_at
            FROM subjects
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopSubjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_study_mode_rows(connection: &Connection) -> Result<Vec<DesktopStudyModeRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval, phase, cycle_index,
                   started_at, phase_started_at, paused_at, total_paused_seconds,
                   phase_paused_seconds, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, ended_at, current_session_id, status,
                   finish_reason, created_at, updated_at, schedule_block_id, today_plan_item_id
            FROM study_modes
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopStudyModeRow {
                id: row.get(0)?,
                state_revision: row.get(1)?,
                mode: row.get(2)?,
                subject_id: row.get(3)?,
                planned_seconds: row.get(4)?,
                focus_seconds: row.get(5)?,
                break_seconds: row.get(6)?,
                long_break_seconds: row.get(7)?,
                long_break_interval: row.get(8)?,
                phase: row.get(9)?,
                cycle_index: row.get(10)?,
                started_at: row.get(11)?,
                phase_started_at: row.get(12)?,
                paused_at: row.get(13)?,
                total_paused_seconds: row.get(14)?,
                _phase_paused_seconds: row.get(15)?,
                accumulated_study_seconds: row.get(16)?,
                paused_stage_elapsed_seconds: row.get(17)?,
                ended_at: row.get(18)?,
                current_session_id: row.get(19)?,
                status: row.get(20)?,
                finish_reason: row.get(21)?,
                created_at: row.get(22)?,
                updated_at: row.get(23)?,
                schedule_block_id: row.get(24)?,
                today_plan_item_id: row.get(25)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_focus_session_rows(connection: &Connection) -> Result<Vec<DesktopFocusSessionRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count, paused_seconds,
                   followed_by_break_type, created_at, updated_at, schedule_block_id, today_plan_item_id
            FROM focus_sessions
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopFocusSessionRow {
                id: row.get(0)?,
                mode: row.get(1)?,
                subject_id: row.get(2)?,
                planned_seconds: row.get(3)?,
                actual_seconds: row.get(4)?,
                started_at: row.get(5)?,
                ended_at: row.get(6)?,
                status: row.get(7)?,
                end_reason: row.get(8)?,
                interruption_count: row.get(9)?,
                emergency_exit_count: row.get(10)?,
                paused_seconds: row.get(11)?,
                followed_by_break_type: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
                schedule_block_id: row.get(15)?,
                today_plan_item_id: row.get(16)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_app_event_rows(connection: &Connection) -> Result<Vec<DesktopAppEventRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, session_id, process_name, window_title, event_type, action_taken, created_at
            FROM app_events
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopAppEventRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                process_name: row.get(2)?,
                window_title: row.get(3)?,
                event_type: row.get(4)?,
                action_taken: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_checklist_task_rows(
    connection: &Connection,
) -> Result<Vec<DesktopChecklistTaskRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed, created_at, updated_at
            FROM checklist_tasks
            WHERE completed IN (0, 1)
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopChecklistTaskRow {
                id: row.get(0)?,
                board_scope: row.get(1)?,
                subject_id: row.get(2)?,
                title: row.get(3)?,
                note: row.get(4)?,
                due_date: row.get(5)?,
                sort_order: row.get(6)?,
                completed: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_today_plan_item_rows(
    connection: &Connection,
) -> Result<Vec<DesktopTodayPlanItemRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            FROM today_plan_items
            ORDER BY today_date ASC, sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopTodayPlanItemRow {
                id: row.get(0)?,
                today_date: row.get(1)?,
                source_task_id: row.get(2)?,
                subject_id: row.get(3)?,
                title: row.get(4)?,
                note: row.get(5)?,
                due_date: row.get(6)?,
                sort_order: row.get(7)?,
                completed: row.get::<_, i64>(8)? != 0,
                synced_source_completion: row.get::<_, i64>(9)? != 0,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_schedule_block_rows(
    connection: &Connection,
) -> Result<Vec<DesktopScheduleBlockRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, schedule_date, title, note, category_key, subject_id, source_today_item_id,
                   template_id, start_minute, end_minute, status, linked_study_mode_id,
                   linked_focus_session_id, created_at, updated_at
            FROM schedule_blocks
            ORDER BY schedule_date ASC, start_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopScheduleBlockRow {
                id: row.get(0)?,
                schedule_date: row.get(1)?,
                title: row.get(2)?,
                note: row.get(3)?,
                category_key: row.get(4)?,
                subject_id: row.get(5)?,
                source_today_item_id: row.get(6)?,
                template_id: row.get(7)?,
                start_minute: row.get(8)?,
                end_minute: row.get(9)?,
                status: row.get(10)?,
                linked_study_mode_id: row.get(11)?,
                linked_focus_session_id: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_schedule_template_rows(
    connection: &Connection,
) -> Result<Vec<DesktopScheduleTemplateRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, note, category_key, subject_id, weekdays, start_minute,
                   end_minute, enabled, created_at, updated_at
            FROM schedule_templates
            ORDER BY start_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopScheduleTemplateRow {
                id: row.get(0)?,
                title: row.get(1)?,
                note: row.get(2)?,
                category_key: row.get(3)?,
                subject_id: row.get(4)?,
                weekdays: row.get(5)?,
                start_minute: row.get(6)?,
                end_minute: row.get(7)?,
                enabled: row.get::<_, i64>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_daily_review_rows(connection: &Connection) -> Result<Vec<DesktopDailyReviewRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            FROM daily_reviews
            ORDER BY review_date ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopDailyReviewRow {
                id: row.get(0)?,
                review_date: row.get(1)?,
                summary: row.get(2)?,
                blockers: row.get(3)?,
                tomorrow_focus: row.get(4)?,
                mood_score: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_weekly_review_rows(connection: &Connection) -> Result<Vec<DesktopWeeklyReviewRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            FROM weekly_reviews
            ORDER BY week_start_date ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopWeeklyReviewRow {
                id: row.get(0)?,
                week_start_date: row.get(1)?,
                summary: row.get(2)?,
                blockers: row.get(3)?,
                next_week_focus: row.get(4)?,
                mood_score: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn ensure_checklist_column(connection: &Connection, board_scope: &str) -> Result<(), String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM checklist_columns WHERE board_scope = ?1",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    if count > 0 {
        return Ok(());
    }

    let now = millis_to_rfc3339(Utc::now().timestamp_millis());
    connection
        .execute(
            "
            INSERT INTO checklist_columns (board_scope, name, sort_order, created_at, updated_at)
            VALUES (?1, 'Default', 0, ?2, ?2)
            ",
            params![board_scope, now],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn get_first_checklist_column_id(
    connection: &Connection,
    board_scope: &str,
) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT id
            FROM checklist_columns
            WHERE board_scope = ?1
            ORDER BY sort_order ASC, id ASC
            LIMIT 1
            ",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn upsert_subject_row(
    connection: &Connection,
    sync_id: &str,
    name: &str,
    color: Option<String>,
    enabled: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) = resolve_local_id_by_sync_id(connection, ENTITY_SUBJECT, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE subjects
                SET name = ?1,
                    color = ?2,
                    enabled = ?3,
                    created_at = ?4,
                    updated_at = ?5
                WHERE id = ?6
                ",
                params![
                    name,
                    color,
                    if enabled { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SUBJECT,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO subjects (name, color, enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                name,
                color,
                if enabled { 1 } else { 0 },
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SUBJECT,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_study_mode_row(
    connection: &Connection,
    sync_id: &str,
    state_revision: i64,
    mode: &str,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    phase: &str,
    round_number: i64,
    started_at: &str,
    phase_started_at: &str,
    paused_at: Option<String>,
    accumulated_study_seconds: i64,
    total_paused_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    status: &str,
    finish_reason: Option<String>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_STUDY_MODE, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE study_modes
                SET state_revision = ?1,
                    mode = ?2,
                    subject_id = ?3,
                    planned_seconds = ?4,
                    focus_seconds = ?5,
                    break_seconds = ?6,
                    long_break_seconds = ?7,
                    long_break_interval = ?8,
                    phase = ?9,
                    cycle_index = ?10,
                    started_at = ?11,
                    phase_started_at = ?12,
                    paused_at = ?13,
                    accumulated_study_seconds = ?14,
                    total_paused_seconds = ?15,
                    phase_paused_seconds = ?16,
                    paused_stage_elapsed_seconds = ?16,
                    ended_at = ?17,
                    current_session_id = ?18,
                    schedule_block_id = ?19,
                    today_plan_item_id = ?20,
                    status = ?21,
                    finish_reason = ?22,
                    created_at = ?23,
                    updated_at = ?24
                WHERE id = ?25
                ",
                params![
                    state_revision.max(1),
                    mode,
                    subject_id,
                    planned_seconds,
                    focus_seconds,
                    break_seconds,
                    long_break_seconds,
                    long_break_interval,
                    phase,
                    round_number,
                    started_at,
                    phase_started_at,
                    paused_at,
                    accumulated_study_seconds,
                    total_paused_seconds,
                    paused_stage_elapsed_seconds,
                    ended_at,
                    current_session_id,
                    schedule_block_id,
                    today_plan_item_id,
                    status,
                    finish_reason,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_STUDY_MODE,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO study_modes (
              state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
              long_break_seconds, long_break_interval, phase, cycle_index,
              started_at, phase_started_at, paused_at, accumulated_study_seconds,
              total_paused_seconds, phase_paused_seconds, paused_stage_elapsed_seconds,
              ended_at, current_session_id, schedule_block_id,
              today_plan_item_id, status, finish_reason, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            ",
            params![
                state_revision.max(1),
                mode,
                subject_id,
                planned_seconds,
                focus_seconds,
                break_seconds,
                long_break_seconds,
                long_break_interval,
                phase,
                round_number,
                started_at,
                phase_started_at,
                paused_at,
                accumulated_study_seconds,
                total_paused_seconds,
                paused_stage_elapsed_seconds,
                paused_stage_elapsed_seconds,
                ended_at,
                current_session_id,
                schedule_block_id,
                today_plan_item_id,
                status,
                finish_reason,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_STUDY_MODE,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_focus_session_row(
    connection: &Connection,
    sync_id: &str,
    mode: &str,
    subject_id: Option<i64>,
    planned_seconds: i64,
    actual_seconds: i64,
    started_at: &str,
    ended_at: Option<String>,
    status: &str,
    end_reason: Option<String>,
    interruption_count: i64,
    emergency_exit_count: i64,
    paused_seconds: i64,
    followed_by_break_type: Option<String>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_FOCUS_SESSION, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE focus_sessions
                SET mode = ?1,
                    subject_id = ?2,
                    planned_seconds = ?3,
                    actual_seconds = ?4,
                    started_at = ?5,
                    ended_at = ?6,
                    status = ?7,
                    end_reason = ?8,
                    interruption_count = ?9,
                    emergency_exit_count = ?10,
                    paused_seconds = ?11,
                    followed_by_break_type = ?12,
                    schedule_block_id = ?13,
                    today_plan_item_id = ?14,
                    created_at = ?15,
                    updated_at = ?16
                WHERE id = ?17
                ",
                params![
                    mode,
                    subject_id,
                    planned_seconds,
                    actual_seconds,
                    started_at,
                    ended_at,
                    status,
                    end_reason,
                    interruption_count,
                    emergency_exit_count,
                    paused_seconds,
                    followed_by_break_type,
                    schedule_block_id,
                    today_plan_item_id,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_FOCUS_SESSION,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO focus_sessions (
              mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
              status, end_reason, interruption_count, emergency_exit_count,
              paused_seconds, followed_by_break_type, schedule_block_id, today_plan_item_id,
              created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ",
            params![
                mode,
                subject_id,
                planned_seconds,
                actual_seconds,
                started_at,
                ended_at,
                status,
                end_reason,
                interruption_count,
                emergency_exit_count,
                paused_seconds,
                followed_by_break_type,
                schedule_block_id,
                today_plan_item_id,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_FOCUS_SESSION,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_local_active_conflicts(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, updated_at, current_session_id
            FROM study_modes
            WHERE status = 'active'
            ORDER BY updated_at DESC, id DESC
            ",
        )
        .map_err(|error| error.to_string())?;
    let active_modes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    if active_modes.len() <= 1 {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    for (mode_id, _, current_session_id) in active_modes.into_iter().skip(1) {
        if let Some(session_id) = current_session_id {
            connection
                .execute(
                    "
                    UPDATE focus_sessions
                    SET status = 'finished',
                        ended_at = COALESCE(ended_at, ?1),
                        end_reason = COALESCE(end_reason, 'sync_takeover'),
                        updated_at = ?1
                    WHERE id = ?2 AND status = 'running'
                    ",
                    params![&now, session_id],
                )
                .map_err(|error| error.to_string())?;
        }

        connection
            .execute(
                "
                UPDATE study_modes
                SET status = 'finished',
                    phase = 'finished',
                    state_revision = state_revision + 1,
                    ended_at = COALESCE(ended_at, ?1),
                    current_session_id = NULL,
                    finish_reason = COALESCE(finish_reason, 'sync_takeover'),
                    updated_at = ?1
                WHERE id = ?2 AND status = 'active'
                ",
                params![&now, mode_id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn upsert_app_event_row(
    connection: &Connection,
    sync_id: &str,
    focus_session_id: Option<i64>,
    package_name: &str,
    app_name: Option<String>,
    event_type: &str,
    action: Option<String>,
    created_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_APP_EVENT, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE app_events
                SET session_id = COALESCE(?1, session_id),
                    process_name = ?2,
                    process_path = NULL,
                    window_title = ?3,
                    event_type = ?4,
                    action_taken = ?5,
                    created_at = ?6
                WHERE id = ?7
                ",
                params![
                    focus_session_id,
                    package_name,
                    app_name,
                    event_type,
                    action,
                    created_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_APP_EVENT,
            local_id,
            sync_id,
            parse_rfc3339_millis(created_at)?,
            None,
        )?;
        return Ok(());
    }

    let Some(session_id) = focus_session_id else {
        return Ok(());
    };

    connection
        .execute(
            "
            INSERT INTO app_events (
              session_id, process_name, process_path, window_title, event_type, action_taken, created_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)
            ",
            params![session_id, package_name, app_name, event_type, action, created_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_APP_EVENT,
        local_id,
        sync_id,
        parse_rfc3339_millis(created_at)?,
        None,
    )
}

fn upsert_checklist_task_row(
    connection: &Connection,
    sync_id: &str,
    board_scope: &str,
    subject_id: Option<i64>,
    column_id: i64,
    title: &str,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_CHECKLIST_TASK, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE checklist_tasks
                SET board_scope = ?1,
                    subject_id = ?2,
                    column_id = ?3,
                    title = ?4,
                    note = ?5,
                    due_date = ?6,
                    sort_order = ?7,
                    completed = ?8,
                    created_at = ?9,
                    updated_at = ?10
                WHERE id = ?11
                ",
                params![
                    board_scope,
                    subject_id,
                    column_id,
                    title,
                    note,
                    due_date,
                    sort_order,
                    if completed { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_CHECKLIST_TASK,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO checklist_tasks (
              board_scope, subject_id, column_id, title, note, due_date, sort_order, completed, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![board_scope, subject_id, column_id, title, note, due_date, sort_order, if completed { 1 } else { 0 }, created_at, updated_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_CHECKLIST_TASK,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_today_plan_item_row(
    connection: &Connection,
    sync_id: &str,
    today_date: &str,
    source_task_id: Option<i64>,
    subject_id: Option<i64>,
    title: &str,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    synced_source_completion: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_TODAY_PLAN_ITEM, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE today_plan_items
                SET today_date = ?1,
                    source_task_id = ?2,
                    subject_id = ?3,
                    title = ?4,
                    note = ?5,
                    due_date = ?6,
                    sort_order = ?7,
                    completed = ?8,
                    synced_source_completion = ?9,
                    created_at = ?10,
                    updated_at = ?11
                WHERE id = ?12
                ",
                params![
                    today_date,
                    source_task_id,
                    subject_id,
                    title,
                    note,
                    due_date,
                    sort_order,
                    if completed { 1 } else { 0 },
                    if synced_source_completion { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO today_plan_items (
              today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            params![today_date, source_task_id, subject_id, title, note, due_date, sort_order, if completed { 1 } else { 0 }, if synced_source_completion { 1 } else { 0 }, created_at, updated_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_TODAY_PLAN_ITEM,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_schedule_template_row(
    connection: &Connection,
    sync_id: &str,
    title: &str,
    note: Option<String>,
    category_key: &str,
    subject_id: Option<i64>,
    weekdays: &str,
    start_minute: i64,
    end_minute: i64,
    enabled: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_SCHEDULE_TEMPLATE, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE schedule_templates
                SET title = ?1,
                    note = ?2,
                    category_key = ?3,
                    subject_id = ?4,
                    weekdays = ?5,
                    start_minute = ?6,
                    end_minute = ?7,
                    enabled = ?8,
                    created_at = ?9,
                    updated_at = ?10
                WHERE id = ?11
                ",
                params![
                    title,
                    note,
                    category_key,
                    subject_id,
                    weekdays,
                    start_minute,
                    end_minute,
                    if enabled { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO schedule_templates (
              title, note, category_key, subject_id, weekdays, start_minute, end_minute,
              enabled, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                title,
                note,
                category_key,
                subject_id,
                weekdays,
                start_minute,
                end_minute,
                if enabled { 1 } else { 0 },
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SCHEDULE_TEMPLATE,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_schedule_block_row(
    connection: &Connection,
    sync_id: &str,
    schedule_date: &str,
    title: &str,
    note: Option<String>,
    category_key: &str,
    subject_id: Option<i64>,
    source_today_item_id: Option<i64>,
    template_id: Option<i64>,
    start_minute: i64,
    end_minute: i64,
    status: &str,
    linked_study_mode_id: Option<i64>,
    linked_focus_session_id: Option<i64>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_schedule_block_import_id(connection, sync_id, schedule_date, template_id)?
    {
        connection
            .execute(
                "
                UPDATE schedule_blocks
                SET schedule_date = ?1,
                    title = ?2,
                    note = ?3,
                    category_key = ?4,
                    subject_id = ?5,
                    source_today_item_id = ?6,
                    template_id = ?7,
                    start_minute = ?8,
                    end_minute = ?9,
                    status = ?10,
                    linked_study_mode_id = ?11,
                    linked_focus_session_id = ?12,
                    created_at = ?13,
                    updated_at = ?14
                WHERE id = ?15
                ",
                params![
                    schedule_date,
                    title,
                    note,
                    category_key,
                    subject_id,
                    source_today_item_id,
                    template_id,
                    start_minute,
                    end_minute,
                    status,
                    linked_study_mode_id,
                    linked_focus_session_id,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              template_id, start_minute, end_minute, status, linked_study_mode_id,
              linked_focus_session_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ",
            params![
                schedule_date,
                title,
                note,
                category_key,
                subject_id,
                source_today_item_id,
                template_id,
                start_minute,
                end_minute,
                status,
                linked_study_mode_id,
                linked_focus_session_id,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_schedule_block_import_id(
    connection: &Connection,
    sync_id: &str,
    schedule_date: &str,
    template_id: Option<i64>,
) -> Result<Option<i64>, String> {
    let by_template_date = match template_id {
        Some(template_id) => connection
            .query_row(
                "
                SELECT id
                FROM schedule_blocks
                WHERE template_id = ?1 AND schedule_date = ?2
                LIMIT 1
                ",
                params![template_id, schedule_date],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?,
        None => None,
    };

    Ok(by_template_date.or(resolve_local_id_by_sync_id(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        Some(sync_id),
    )?))
}

fn upsert_daily_review_row(
    connection: &Connection,
    sync_id: &str,
    review_date: &str,
    summary: Option<String>,
    blockers: Option<String>,
    tomorrow_focus: Option<String>,
    mood_score: i64,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let existing_id = resolve_local_id_by_sync_id(connection, ENTITY_DAILY_REVIEW, Some(sync_id))?
        .or_else(|| {
            connection
                .query_row(
                    "SELECT id FROM daily_reviews WHERE review_date = ?1",
                    params![review_date],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .ok()
                .flatten()
        });

    if let Some(local_id) = existing_id {
        connection
            .execute(
                "
                UPDATE daily_reviews
                SET review_date = ?1,
                    summary = ?2,
                    blockers = ?3,
                    tomorrow_focus = ?4,
                    mood_score = ?5,
                    created_at = ?6,
                    updated_at = ?7
                WHERE id = ?8
                ",
                params![
                    review_date,
                    summary,
                    blockers,
                    tomorrow_focus,
                    mood_score,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_DAILY_REVIEW,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO daily_reviews (
              review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                review_date,
                summary,
                blockers,
                tomorrow_focus,
                mood_score,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_DAILY_REVIEW,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn upsert_weekly_review_row(
    connection: &Connection,
    sync_id: &str,
    week_start_date: &str,
    summary: Option<String>,
    blockers: Option<String>,
    next_week_focus: Option<String>,
    mood_score: i64,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let existing_id = resolve_local_id_by_sync_id(connection, ENTITY_WEEKLY_REVIEW, Some(sync_id))?
        .or_else(|| {
            connection
                .query_row(
                    "SELECT id FROM weekly_reviews WHERE week_start_date = ?1",
                    params![week_start_date],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .ok()
                .flatten()
        });

    if let Some(local_id) = existing_id {
        connection
            .execute(
                "
                UPDATE weekly_reviews
                SET week_start_date = ?1,
                    summary = ?2,
                    blockers = ?3,
                    next_week_focus = ?4,
                    mood_score = ?5,
                    created_at = ?6,
                    updated_at = ?7
                WHERE id = ?8
                ",
                params![
                    week_start_date,
                    summary,
                    blockers,
                    next_week_focus,
                    mood_score,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_WEEKLY_REVIEW,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO weekly_reviews (
              week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                week_start_date,
                summary,
                blockers,
                next_week_focus,
                mood_score,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_WEEKLY_REVIEW,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_or_create_sync_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    preferred_sync_id: Option<String>,
    updated_at: i64,
) -> Result<String, String> {
    if let Some(meta) = get_sync_meta_by_local_id(connection, entity_type, local_id)? {
        upsert_sync_meta(
            connection,
            entity_type,
            local_id,
            &meta.sync_id,
            updated_at,
            None,
        )?;
        return Ok(meta.sync_id);
    }

    let fallback_sync_id = format!("{entity_type}-{local_id}");
    let sync_id = match preferred_sync_id {
        Some(preferred) => match get_sync_meta_by_sync_id(connection, &preferred)? {
            Some(existing) if existing.local_id != local_id => fallback_sync_id,
            _ => preferred,
        },
        None => fallback_sync_id,
    };
    upsert_sync_meta(
        connection,
        entity_type,
        local_id,
        &sync_id,
        updated_at,
        None,
    )?;
    Ok(sync_id)
}

fn upsert_sync_meta(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    sync_id: &str,
    updated_at: i64,
    deleted_at: Option<i64>,
) -> Result<(), String> {
    if let Some(existing) = get_sync_meta_by_sync_id(connection, sync_id)? {
        connection
            .execute(
                "
                UPDATE sync_meta
                SET entity_type = ?1,
                    local_id = ?2,
                    deleted_at = ?3,
                    updated_at = ?4
                WHERE sync_id = ?5
                ",
                params![entity_type, local_id, deleted_at, updated_at, sync_id],
            )
            .map_err(|error| error.to_string())?;
        let _ = existing;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(entity_type, local_id) DO UPDATE SET
                sync_id = excluded.sync_id,
                deleted_at = excluded.deleted_at,
                updated_at = excluded.updated_at
            ",
            params![entity_type, local_id, sync_id, deleted_at, updated_at],
        )
        .or_else(|error| {
            if !error
                .to_string()
                .contains("UNIQUE constraint failed: sync_meta.sync_id")
            {
                return Err(error);
            }
            connection.execute(
                "DELETE FROM sync_meta WHERE sync_id = ?1 OR (entity_type = ?2 AND local_id = ?3)",
                params![sync_id, entity_type, local_id],
            )?;
            connection.execute(
                "
                INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![entity_type, local_id, sync_id, deleted_at, updated_at],
            )?;
            Ok(1)
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_sync_meta_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
) -> Result<Option<SyncMetaRow>, String> {
    connection
        .query_row(
            "
            SELECT local_id, sync_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND local_id = ?2
            ",
            params![entity_type, local_id],
            |row| {
                Ok(SyncMetaRow {
                    local_id: row.get(0)?,
                    sync_id: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_sync_meta_by_sync_id(
    connection: &Connection,
    sync_id: &str,
) -> Result<Option<SyncMetaRow>, String> {
    connection
        .query_row(
            "
            SELECT local_id, sync_id, deleted_at
            FROM sync_meta
            WHERE sync_id = ?1
            ",
            params![sync_id],
            |row| {
                Ok(SyncMetaRow {
                    local_id: row.get(0)?,
                    sync_id: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn delete_local_row_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: &str,
    deleted_at: i64,
) -> Result<(), String> {
    let local_id = resolve_local_id_by_sync_id(connection, entity_type, Some(sync_id))?;
    if let Some(local_id) = local_id {
        match entity_type {
            ENTITY_SUBJECT => {
                connection
                    .execute("DELETE FROM subjects WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_STUDY_MODE => {
                connection
                    .execute(
                        "
                        UPDATE schedule_blocks
                        SET linked_study_mode_id = NULL
                        WHERE linked_study_mode_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute("DELETE FROM study_modes WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_FOCUS_SESSION => {
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET current_session_id = NULL
                        WHERE current_session_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "
                        UPDATE schedule_blocks
                        SET linked_focus_session_id = NULL
                        WHERE linked_focus_session_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM app_events WHERE session_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM focus_sessions WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_APP_EVENT => {
                connection
                    .execute("DELETE FROM app_events WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_CHECKLIST_TASK => {
                connection
                    .execute(
                        "DELETE FROM today_plan_items WHERE source_task_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM checklist_tasks WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_TODAY_PLAN_ITEM => {
                connection
                    .execute(
                        "DELETE FROM today_plan_items WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_SCHEDULE_BLOCK => {
                connection
                    .execute(
                        "DELETE FROM schedule_blocks WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_SCHEDULE_TEMPLATE => {
                connection
                    .execute(
                        "DELETE FROM schedule_blocks WHERE template_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM schedule_templates WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_DAILY_REVIEW => {
                connection
                    .execute("DELETE FROM daily_reviews WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_WEEKLY_REVIEW => {
                connection
                    .execute(
                        "DELETE FROM weekly_reviews WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            _ => {}
        }
    }

    let tombstone_local_id =
        local_id.unwrap_or_else(|| synthetic_local_id_for_sync_id(entity_type, sync_id));
    upsert_sync_meta(
        connection,
        entity_type,
        tombstone_local_id,
        sync_id,
        deleted_at,
        Some(deleted_at),
    )?;
    Ok(())
}

fn resolve_local_id_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: Option<&str>,
) -> Result<Option<i64>, String> {
    let Some(sync_id) = sync_id else {
        return Ok(None);
    };
    let sync_id = if entity_type == ENTITY_SUBJECT {
        canonical_subject_sync_id(sync_id)
    } else {
        sync_id.to_string()
    };

    let meta = get_sync_meta_by_sync_id(connection, &sync_id)?;
    if let Some(meta) = meta {
        if meta.deleted_at.is_some() {
            return Ok(None);
        }
        return Ok(Some(meta.local_id));
    }

    let _ = entity_type;
    Ok(None)
}

fn resolve_existing_local_id_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: Option<&str>,
) -> Result<Option<i64>, String> {
    let Some(local_id) = resolve_local_id_by_sync_id(connection, entity_type, sync_id)? else {
        return Ok(None);
    };

    if local_row_exists(connection, entity_type, local_id)? {
        Ok(Some(local_id))
    } else {
        Ok(None)
    }
}

fn resolve_sync_id_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(local_id) = local_id else {
        return Ok(None);
    };

    let sync_id = connection
        .query_row(
            "
            SELECT sync_id
            FROM sync_meta
            WHERE entity_type = ?1 AND local_id = ?2
            ",
            params![entity_type, local_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(if entity_type == ENTITY_SUBJECT {
        sync_id.map(|value| canonical_subject_sync_id(&value))
    } else {
        sync_id
    })
}

fn resolve_existing_sync_id_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(local_id) = local_id else {
        return Ok(None);
    };
    if !local_row_exists(connection, entity_type, local_id)? {
        return Ok(None);
    }
    resolve_sync_id_by_local_id(connection, entity_type, Some(local_id))
}

fn local_row_exists(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
) -> Result<bool, String> {
    let table = match entity_type {
        ENTITY_SUBJECT => "subjects",
        ENTITY_STUDY_MODE => "study_modes",
        ENTITY_FOCUS_SESSION => "focus_sessions",
        ENTITY_APP_EVENT => "app_events",
        ENTITY_CHECKLIST_TASK => "checklist_tasks",
        ENTITY_TODAY_PLAN_ITEM => "today_plan_items",
        ENTITY_SCHEDULE_BLOCK => "schedule_blocks",
        ENTITY_SCHEDULE_TEMPLATE => "schedule_templates",
        ENTITY_DAILY_REVIEW => "daily_reviews",
        ENTITY_WEEKLY_REVIEW => "weekly_reviews",
        _ => return Ok(false),
    };
    let exists: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1"),
            params![local_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(exists > 0)
}

fn resolve_study_mode_sync_id_for_session(
    connection: &Connection,
    session_id: i64,
) -> Result<Option<String>, String> {
    let study_mode_id = connection
        .query_row(
            "
            SELECT id
            FROM study_modes
            WHERE current_session_id = ?1
            ORDER BY updated_at DESC, id DESC
            LIMIT 1
            ",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    resolve_existing_sync_id_by_local_id(connection, ENTITY_STUDY_MODE, study_mode_id)
}

fn synthetic_local_id_for_sync_id(entity_type: &str, sync_id: &str) -> i64 {
    let mut hasher = DefaultHasher::new();
    entity_type.hash(&mut hasher);
    sync_id.hash(&mut hasher);
    let hash = hasher.finish() as i64;
    let positive = hash.unsigned_abs() as i64;
    if positive == 0 {
        1
    } else {
        positive
    }
}

fn board_scope_for_category_key(category_key: &str) -> String {
    match category_key {
        "politics" => "checklist:politics".to_string(),
        "english" => "checklist:english".to_string(),
        "math" => "checklist:math".to_string(),
        "major" => "checklist:major".to_string(),
        _ => "checklist:general".to_string(),
    }
}

fn map_board_scope_to_category_key(board_scope: &str) -> String {
    match board_scope {
        "checklist:politics" => "politics".to_string(),
        "checklist:english" => "english".to_string(),
        "checklist:math" => "math".to_string(),
        "checklist:major" => "major".to_string(),
        _ => "general".to_string(),
    }
}

fn parse_weekdays_json(raw: &str) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|weekday| matches!(*weekday, 1..=7))
        .collect()
}

fn default_subject_sync_id(name: &str, local_id: i64) -> String {
    canonical_subject_sync_id_for_name(name)
        .map(str::to_string)
        .unwrap_or_else(|| format!("subject-{local_id}"))
}

fn normalize_name(value: &str) -> String {
    value.trim().to_string()
}

fn parse_rfc3339_millis(value: &str) -> Result<i64, String> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|error| error.to_string())?
        .with_timezone(&Utc)
        .timestamp_millis())
}

fn millis_to_rfc3339(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn ensure_device_id(connection: &Connection) -> Result<String, String> {
    let existing = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'sync_device_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(device_id) = existing {
        if !device_id.trim().is_empty() {
            return Ok(device_id);
        }
    }

    let device_id = Uuid::new_v4().to_string();
    let now = millis_to_rfc3339(Utc::now().timestamp_millis());
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES ('sync_device_id', ?1, ?2)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![device_id, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(device_id)
}

pub fn load_or_create_device_id(connection: &Connection) -> Result<String, String> {
    ensure_device_id(connection)
}

pub fn ensure_sync_meta_for_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    preferred_sync_id: Option<String>,
    updated_at: i64,
) -> Result<String, String> {
    resolve_or_create_sync_id(
        connection,
        entity_type,
        local_id,
        preferred_sync_id,
        updated_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_database;
    use tempfile::tempdir;

    fn empty_payload(device_id: &str, exported_at: i64) -> SharedSyncPayload {
        SharedSyncPayload {
            schema_version: SYNC_SCHEMA_VERSION,
            device_id: device_id.to_string(),
            exported_at,
            source_device_id: Some(device_id.to_string()),
            active_device_id: None,
            subjects: Vec::new(),
            study_modes: Vec::new(),
            focus_sessions: Vec::new(),
            app_events: Vec::new(),
            checklist_tasks: Vec::new(),
            today_plan_items: Vec::new(),
            schedule_blocks: Vec::new(),
            schedule_templates: Vec::new(),
            daily_reviews: Vec::new(),
            weekly_reviews: Vec::new(),
        }
    }

    fn running_study_mode(
        sync_id: &str,
        round_number: i64,
        phase: &str,
        accumulated_study_seconds: i64,
        phase_started_at: i64,
        updated_at: i64,
    ) -> SharedStudyMode {
        SharedStudyMode {
            sync_id: sync_id.to_string(),
            state_revision: Some(round_number.max(1)),
            mode: Some("normal".to_string()),
            subject_sync_id: None,
            planned_seconds: Some(3600),
            focus_seconds: Some(1500),
            break_seconds: Some(300),
            long_break_seconds: Some(900),
            long_break_interval: Some(4),
            phase: Some(phase.to_string()),
            round_number: Some(round_number),
            started_at: Some(0),
            phase_started_at: Some(phase_started_at),
            paused_at: None,
            paused_from_phase: None,
            accumulated_study_seconds: Some(accumulated_study_seconds),
            total_paused_seconds: Some(0),
            phase_paused_seconds: Some(0),
            paused_stage_elapsed_seconds: Some(0),
            current_break_type: Some("short".to_string()),
            ended_at: None,
            current_session_sync_id: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            status: Some("running".to_string()),
            finish_reason: None,
            created_at: Some(0),
            updated_at,
            deleted_at: None,
        }
    }

    fn focus_session(
        sync_id: &str,
        study_mode_sync_id: &str,
        status: &str,
        updated_at: i64,
    ) -> SharedFocusSession {
        SharedFocusSession {
            sync_id: sync_id.to_string(),
            study_mode_sync_id: Some(study_mode_sync_id.to_string()),
            subject_sync_id: None,
            mode: Some("normal".to_string()),
            planned_seconds: Some(1500),
            actual_seconds: Some(0),
            started_at: Some(0),
            ended_at: None,
            status: Some(status.to_string()),
            end_reason: None,
            interruption_count: Some(0),
            emergency_exit_count: Some(0),
            paused_seconds: Some(0),
            followed_by_break_type: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            created_at: Some(0),
            updated_at,
            deleted_at: None,
        }
    }

    #[test]
    fn checklist_tombstone_wins_over_same_timestamp_live_item() {
        let mut local = empty_payload("desktop", 1000);
        local.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-1".to_string(),
            category_key: Some("math".to_string()),
            subject_sync_id: None,
            title: Some("old".to_string()),
            note: None,
            due_date: None,
            sort_order: Some(0.0),
            completed: Some(false),
            created_at: Some(900),
            updated_at: 2000,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-1".to_string(),
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: 2000,
            deleted_at: Some(2000),
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.checklist_tasks.len(), 1);
        assert_eq!(merged.checklist_tasks[0].deleted_at, Some(2000));
    }

    #[test]
    fn newer_tombstone_wins_over_older_schedule_block() {
        let mut local = empty_payload("desktop", 1000);
        local.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "block-1".to_string(),
            schedule_date: Some("2026-05-18".to_string()),
            title: Some("math".to_string()),
            note: None,
            category_key: Some("math".to_string()),
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: None,
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 1500,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "block-1".to_string(),
            schedule_date: None,
            title: None,
            note: None,
            category_key: None,
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: None,
            start_minute: None,
            end_minute: None,
            status: None,
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: None,
            updated_at: 2500,
            deleted_at: Some(2500),
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.schedule_blocks.len(), 1);
        assert_eq!(merged.schedule_blocks[0].deleted_at, Some(2500));
    }

    #[test]
    fn active_study_mode_never_rolls_back_to_older_round_even_with_newer_timestamp() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "mode-1", 2, "focus", 1500, 50_000, 2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 0, 10_000, 3_000));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].round_number, Some(2));
        assert_eq!(merged.study_modes[0].accumulated_study_seconds, Some(1500));
    }

    #[test]
    fn active_study_mode_accepts_remote_pause_command() {
        let mut local = empty_payload("desktop", 60_000);
        local
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 100, 50_000, 2_000));
        let mut remote = empty_payload("phone", 60_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 100, 50_000, 3_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        paused.paused_stage_elapsed_seconds = Some(5);
        remote.study_modes.push(paused);

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("paused"));
        assert_eq!(merged.study_modes[0].paused_at, Some(55_000));
    }

    #[test]
    fn active_study_mode_accepts_remote_resume_command() {
        let mut local = empty_payload("desktop", 60_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 100, 50_000, 2_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        local.study_modes.push(paused);
        let mut remote = empty_payload("phone", 60_000);
        remote
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 100, 56_000, 3_000));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("focus"));
        assert_eq!(merged.study_modes[0].paused_at, None);
    }

    #[test]
    fn active_study_mode_accepts_remote_start_break_command() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "mode-1",
            1,
            "awaiting_break",
            1500,
            50_000,
            2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.study_modes.push(running_study_mode(
            "mode-1", 1, "break", 1500, 55_000, 3_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("break"));
    }

    #[test]
    fn active_study_mode_accepts_remote_finish_command_and_session() {
        let mut local = empty_payload("desktop", 60_000);
        let mut local_mode = running_study_mode("mode-1", 1, "focus", 100, 50_000, 2_000);
        local_mode.current_session_sync_id = Some("session-1".to_string());
        local.study_modes.push(local_mode);
        local
            .focus_sessions
            .push(focus_session("session-1", "mode-1", "running", 2_000));

        let mut remote = empty_payload("phone", 60_000);
        let mut finished = running_study_mode("mode-1", 1, "finished", 120, 50_000, 3_000);
        finished.status = Some("finished".to_string());
        finished.ended_at = Some(58_000);
        finished.current_session_sync_id = Some("session-1".to_string());
        remote.study_modes.push(finished);
        let mut session = focus_session("session-1", "mode-1", "finished", 3_000);
        session.actual_seconds = Some(120);
        session.ended_at = Some(58_000);
        remote.focus_sessions.push(session);

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].status.as_deref(), Some("finished"));
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("finished"));
        assert_eq!(merged.focus_sessions.len(), 1);
        assert_eq!(merged.focus_sessions[0].status.as_deref(), Some("finished"));
        assert_eq!(merged.focus_sessions[0].actual_seconds, Some(120));
    }

    #[test]
    fn different_remote_active_does_not_take_over_local_active() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "desktop-mode",
            1,
            "focus",
            300,
            50_000,
            2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.study_modes.push(running_study_mode(
            "phone-mode",
            1,
            "focus",
            0,
            59_000,
            3_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        let desktop_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "desktop-mode")
            .expect("desktop active should remain");
        let phone_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "phone-mode")
            .expect("phone mode should be retained as history");
        assert_eq!(desktop_mode.status.as_deref(), Some("running"));
        assert_eq!(desktop_mode.phase.as_deref(), Some("focus"));
        assert_eq!(phone_mode.status.as_deref(), Some("finished"));
        assert_eq!(phone_mode.finish_reason.as_deref(), Some("sync_takeover"));
    }

    #[test]
    fn schedule_blocks_with_same_stable_sync_id_do_not_duplicate() {
        let mut local = empty_payload("desktop", 1000);
        local.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "schedule_block:2026-05-20:template:7:480-540".to_string(),
            schedule_date: Some("2026-05-20".to_string()),
            title: Some("math".to_string()),
            note: None,
            category_key: Some("math".to_string()),
            subject_sync_id: Some("subject-3".to_string()),
            source_today_item_sync_id: None,
            template_sync_id: Some("schedule_template-7".to_string()),
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 1000,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "schedule_block:2026-05-20:template:7:480-540".to_string(),
            schedule_date: Some("2026-05-20".to_string()),
            title: Some("math".to_string()),
            note: Some("updated".to_string()),
            category_key: Some("math".to_string()),
            subject_sync_id: Some("subject-3".to_string()),
            source_today_item_sync_id: None,
            template_sync_id: Some("schedule_template-7".to_string()),
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 2000,
            deleted_at: None,
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.schedule_blocks.len(), 1);
        assert_eq!(merged.schedule_blocks[0].note.as_deref(), Some("updated"));
    }

    #[test]
    fn importing_remote_tombstone_preserves_remote_deleted_at() {
        let directory = tempdir().expect("create temp directory");
        let mut connection =
            open_database(&directory.path().join("sync-test.sqlite3")).expect("open test db");
        let mut payload = empty_payload("phone", 5_000);
        payload.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-deleted-remotely".to_string(),
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: 2_000,
            deleted_at: Some(2_000),
        });

        import_shared_sync_payload(&mut connection, &payload).expect("import payload");

        let (deleted_at, updated_at): (i64, i64) = connection
            .query_row(
                "SELECT deleted_at, updated_at FROM sync_meta WHERE sync_id = ?1",
                params!["task-deleted-remotely"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read tombstone meta");
        assert_eq!(deleted_at, 2_000);
        assert_eq!(updated_at, 2_000);
    }

    #[test]
    fn importing_schedule_block_reuses_existing_template_date_row() {
        let directory = tempdir().expect("create temp directory");
        let mut connection =
            open_database(&directory.path().join("sync-test.sqlite3")).expect("open test db");
        upsert_schedule_template_row(
            &connection,
            "template-1",
            "template",
            None,
            "general",
            None,
            "[1]",
            480,
            540,
            true,
            &millis_to_rfc3339(1_000),
            &millis_to_rfc3339(1_000),
        )
        .expect("insert template");
        let template_id: i64 = connection
            .query_row(
                "SELECT id FROM schedule_templates WHERE title = ?1",
                params!["template"],
                |row| row.get(0),
            )
            .expect("read template id");
        upsert_schedule_block_row(
            &connection,
            "local-block",
            "2030-02-01",
            "old block",
            None,
            "general",
            None,
            None,
            Some(template_id),
            480,
            540,
            "planned",
            None,
            None,
            &millis_to_rfc3339(1_000),
            &millis_to_rfc3339(1_000),
        )
        .expect("insert local generated block");

        let mut payload = empty_payload("phone", 2_000);
        payload.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "remote-block".to_string(),
            schedule_date: Some("2030-02-01".to_string()),
            title: Some("remote block".to_string()),
            note: Some("updated".to_string()),
            category_key: Some("general".to_string()),
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: Some("template-1".to_string()),
            start_minute: Some(500),
            end_minute: Some(560),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(2_000),
            updated_at: 2_000,
            deleted_at: None,
        });

        import_shared_sync_payload(&mut connection, &payload).expect("import payload");

        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schedule_blocks WHERE template_id = ?1 AND schedule_date = ?2",
                params![template_id, "2030-02-01"],
                |row| row.get(0),
            )
            .expect("count blocks");
        let (title, sync_id): (String, String) = connection
            .query_row(
                "
                SELECT b.title, m.sync_id
                FROM schedule_blocks b
                JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
                WHERE b.template_id = ?1 AND b.schedule_date = ?2
                ",
                params![template_id, "2030-02-01"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read merged block");
        assert_eq!(count, 1);
        assert_eq!(title, "remote block");
        assert_eq!(sync_id, "remote-block");
    }
}

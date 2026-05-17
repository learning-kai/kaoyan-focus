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
const DEFAULT_SUBJECT_SYNC_IDS: [&str; 4] = ["subject-1", "subject-2", "subject-3", "subject-4"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSyncPayload {
    pub schema_version: i64,
    pub device_id: String,
    pub exported_at: i64,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedActiveStudySnapshot {
    pub sync_id: String,
    pub status: Option<String>,
    pub phase: Option<String>,
    pub subject_sync_id: Option<String>,
    pub current_session_sync_id: Option<String>,
    pub paused_at: Option<i64>,
    pub round_number: Option<i64>,
    pub current_break_type: Option<String>,
    pub ended_at: Option<i64>,
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
    pub current_break_type: Option<String>,
    pub ended_at: Option<i64>,
    pub current_session_sync_id: Option<String>,
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
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
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
    phase_paused_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
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

pub fn export_shared_sync_payload(
    connection: &Connection,
    device_id: String,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    Ok(SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION,
        device_id,
        exported_at,
        subjects: export_subjects(connection)?,
        study_modes: export_study_modes(connection)?,
        focus_sessions: export_focus_sessions(connection)?,
        app_events: export_app_events(connection)?,
        checklist_tasks: export_checklist_tasks(connection)?,
        today_plan_items: export_today_plan_items(connection)?,
    })
}

pub fn merge_shared_sync_payloads(
    local: SharedSyncPayload,
    remote: SharedSyncPayload,
    device_id: String,
    exported_at: i64,
) -> SharedSyncPayload {
    let mut study_modes = merge_latest_by_sync_id(
        &local.study_modes,
        &remote.study_modes,
        |item| item.sync_id.as_str(),
        |item| item.updated_at,
    );
    let mut focus_sessions = merge_latest_by_sync_id(
        &local.focus_sessions,
        &remote.focus_sessions,
        |item| item.sync_id.as_str(),
        |item| item.updated_at,
    );
    resolve_shared_active_conflicts(&mut study_modes, &mut focus_sessions, exported_at);

    SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION
            .max(local.schema_version)
            .max(remote.schema_version),
        device_id,
        exported_at,
        subjects: merge_latest_by_sync_id(
            &local.subjects,
            &remote.subjects,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
        ),
        study_modes,
        focus_sessions,
        app_events: merge_latest_by_sync_id(
            &local.app_events,
            &remote.app_events,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
        ),
        checklist_tasks: merge_latest_by_sync_id(
            &local.checklist_tasks,
            &remote.checklist_tasks,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
        ),
        today_plan_items: merge_latest_by_sync_id(
            &local.today_plan_items,
            &remote.today_plan_items,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
        ),
    }
}

pub fn shared_active_study_snapshot(
    payload: &SharedSyncPayload,
) -> Option<SharedActiveStudySnapshot> {
    payload
        .study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.sync_id.cmp(&right.sync_id))
        })
        .map(|item| SharedActiveStudySnapshot {
            sync_id: item.sync_id.clone(),
            status: item.status.clone(),
            phase: item.phase.clone(),
            subject_sync_id: item.subject_sync_id.clone(),
            current_session_sync_id: item.current_session_sync_id.clone(),
            paused_at: item.paused_at,
            round_number: item.round_number,
            current_break_type: item.current_break_type.clone(),
            ended_at: item.ended_at,
            updated_at: item.updated_at,
        })
}

pub fn import_shared_sync_payload(
    connection: &mut Connection,
    payload: &SharedSyncPayload,
) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    import_subjects(&transaction, &payload.subjects)?;
    import_focus_sessions(&transaction, &payload.focus_sessions)?;
    import_study_modes(&transaction, &payload.study_modes)?;
    import_app_events(&transaction, &payload.app_events)?;
    import_checklist_tasks(&transaction, &payload.checklist_tasks)?;
    import_today_plan_items(&transaction, &payload.today_plan_items)?;
    resolve_local_active_conflicts(&transaction)?;
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

fn merge_latest_by_sync_id<T, Id, Updated>(
    local: &[T],
    remote: &[T],
    id_of: Id,
    updated_at_of: Updated,
) -> Vec<T>
where
    T: Clone,
    Id: Fn(&T) -> &str,
    Updated: Fn(&T) -> i64,
{
    let mut merged: HashMap<String, T> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = id_of(item).trim();
        if sync_id.is_empty() {
            continue;
        }

        let should_replace = merged
            .get(sync_id)
            .map(|existing| updated_at_of(item) > updated_at_of(existing))
            .unwrap_or(true);
        if should_replace {
            merged.insert(sync_id.to_string(), item.clone());
        }
    }

    merged.into_values().collect()
}

fn resolve_shared_active_conflicts(
    study_modes: &mut [SharedStudyMode],
    focus_sessions: &mut [SharedFocusSession],
    resolved_at: i64,
) {
    let winner_sync_id = study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.sync_id.cmp(&right.sync_id))
        })
        .map(|item| item.sync_id.clone());

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
        mode.updated_at = resolved_at;
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
            session.updated_at = resolved_at;
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

fn millis_to_seconds(millis: i64) -> i64 {
    millis.saturating_div(1000)
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
        let exported_at = Utc::now().timestamp_millis();
        let open_pause_seconds = paused_at_millis
            .map(|paused_at| millis_to_seconds(exported_at.saturating_sub(paused_at)))
            .unwrap_or(0);
        let study_elapsed_seconds =
            millis_to_seconds(exported_at.saturating_sub(started_at_millis))
                .saturating_sub(row.total_paused_seconds)
                .saturating_sub(open_pause_seconds)
                .max(0);
        let phase_end_millis = paused_at_millis.unwrap_or(exported_at);
        let phase_elapsed_seconds =
            millis_to_seconds(phase_end_millis.saturating_sub(phase_started_at_millis))
                .saturating_sub(row.phase_paused_seconds)
                .max(0);
        let accumulated_study_seconds = if row.phase == "focus" || row.phase == "awaiting_break" {
            study_elapsed_seconds.saturating_sub(phase_elapsed_seconds)
        } else {
            study_elapsed_seconds
        };
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
            mode: Some(row.mode),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
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
            accumulated_study_seconds: Some(accumulated_study_seconds),
            total_paused_seconds: Some(row.total_paused_seconds),
            phase_paused_seconds: Some(row.phase_paused_seconds),
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
                resolve_sync_id_by_local_id(connection, ENTITY_FOCUS_SESSION, Some(session_id))
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
                resolve_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
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
            focus_session_sync_id: resolve_sync_id_by_local_id(
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
                resolve_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
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
                resolve_sync_id_by_local_id(connection, ENTITY_CHECKLIST_TASK, Some(source_task_id))
                    .ok()
                    .flatten()
            }),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
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
            current_break_type: None,
            ended_at: None,
            current_session_sync_id: None,
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

fn import_subjects(connection: &Connection, items: &[SharedSubject]) -> Result<(), String> {
    for item in items {
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_SUBJECT, &item.sync_id)?;
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
            &item.sync_id,
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
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_STUDY_MODE, &item.sync_id)?;
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

        let subject_id = resolve_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let current_session_id = resolve_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.current_session_sync_id.as_deref(),
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
            item.total_paused_seconds.unwrap_or(0),
            item.phase_paused_seconds.unwrap_or(0),
            item.ended_at
                .as_ref()
                .map(|value| millis_to_rfc3339(*value)),
            current_session_id,
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
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_FOCUS_SESSION, &item.sync_id)?;
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

        let subject_id = resolve_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
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
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_app_events(connection: &Connection, items: &[SharedAppEvent]) -> Result<(), String> {
    for item in items {
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_APP_EVENT, &item.sync_id)?;
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

        let focus_session_id = resolve_local_id_by_sync_id(
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
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_CHECKLIST_TASK, &item.sync_id)?;
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
        let subject_id = resolve_local_id_by_sync_id(
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
        if item.deleted_at.is_some() {
            delete_local_row_by_sync_id(connection, ENTITY_TODAY_PLAN_ITEM, &item.sync_id)?;
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

        let source_task_id = resolve_local_id_by_sync_id(
            connection,
            ENTITY_CHECKLIST_TASK,
            item.source_task_sync_id.as_deref(),
        )?;
        let subject_id = resolve_local_id_by_sync_id(
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
            SELECT id, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval, phase, cycle_index,
                   started_at, phase_started_at, paused_at, total_paused_seconds,
                   phase_paused_seconds, ended_at, current_session_id, status, finish_reason,
                   created_at, updated_at
            FROM study_modes
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopStudyModeRow {
                id: row.get(0)?,
                mode: row.get(1)?,
                subject_id: row.get(2)?,
                planned_seconds: row.get(3)?,
                focus_seconds: row.get(4)?,
                break_seconds: row.get(5)?,
                long_break_seconds: row.get(6)?,
                long_break_interval: row.get(7)?,
                phase: row.get(8)?,
                cycle_index: row.get(9)?,
                started_at: row.get(10)?,
                phase_started_at: row.get(11)?,
                paused_at: row.get(12)?,
                total_paused_seconds: row.get(13)?,
                phase_paused_seconds: row.get(14)?,
                ended_at: row.get(15)?,
                current_session_id: row.get(16)?,
                status: row.get(17)?,
                finish_reason: row.get(18)?,
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
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
                   followed_by_break_type, created_at, updated_at
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
    total_paused_seconds: i64,
    phase_paused_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
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
                SET mode = ?1,
                    subject_id = ?2,
                    planned_seconds = ?3,
                    focus_seconds = ?4,
                    break_seconds = ?5,
                    long_break_seconds = ?6,
                    long_break_interval = ?7,
                    phase = ?8,
                    cycle_index = ?9,
                    started_at = ?10,
                    phase_started_at = ?11,
                    paused_at = ?12,
                    total_paused_seconds = ?13,
                    phase_paused_seconds = ?14,
                    ended_at = ?15,
                    current_session_id = ?16,
                    status = ?17,
                    finish_reason = ?18,
                    created_at = ?19,
                    updated_at = ?20
                WHERE id = ?21
                ",
                params![
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
                    total_paused_seconds,
                    phase_paused_seconds,
                    ended_at,
                    current_session_id,
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
              mode, subject_id, planned_seconds, focus_seconds, break_seconds,
              long_break_seconds, long_break_interval, phase, cycle_index,
              started_at, phase_started_at, paused_at, total_paused_seconds,
              phase_paused_seconds, ended_at, current_session_id, status,
              finish_reason, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ",
            params![
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
                total_paused_seconds,
                phase_paused_seconds,
                ended_at,
                current_session_id,
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
                    created_at = ?13,
                    updated_at = ?14
                WHERE id = ?15
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
              paused_seconds, followed_by_break_type, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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

    let sync_id = preferred_sync_id.unwrap_or_else(|| format!("{entity_type}-{local_id}"));
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
            ",
            params![entity_type, local_id, sync_id, deleted_at, updated_at],
        )
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
                    .execute("DELETE FROM study_modes WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_FOCUS_SESSION => {
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
            _ => {}
        }
    }

    let tombstone_local_id =
        local_id.unwrap_or_else(|| synthetic_local_id_for_sync_id(entity_type, sync_id));
    let deleted_at = Utc::now().timestamp_millis();
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

    let meta = get_sync_meta_by_sync_id(connection, sync_id)?;
    if let Some(meta) = meta {
        if meta.deleted_at.is_some() {
            return Ok(None);
        }
        return Ok(Some(meta.local_id));
    }

    let _ = entity_type;
    Ok(None)
}

fn resolve_sync_id_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(local_id) = local_id else {
        return Ok(None);
    };

    connection
        .query_row(
            "
            SELECT sync_id
            FROM sync_meta
            WHERE entity_type = ?1 AND local_id = ?2
            ",
            params![entity_type, local_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
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

    resolve_sync_id_by_local_id(connection, ENTITY_STUDY_MODE, study_mode_id)
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

fn default_subject_sync_id(name: &str, local_id: i64) -> String {
    match normalize_name(name).as_str() {
        "政治" => DEFAULT_SUBJECT_SYNC_IDS[0].to_string(),
        "英语" => DEFAULT_SUBJECT_SYNC_IDS[1].to_string(),
        "数学" => DEFAULT_SUBJECT_SYNC_IDS[2].to_string(),
        "专业课" => DEFAULT_SUBJECT_SYNC_IDS[3].to_string(),
        _ => format!("subject-{local_id}"),
    }
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

use crate::{
    commands::focus::{self, StudyModeLinks, StudyModeState},
    storage::db::open_database,
    sync_package::{ensure_sync_meta_for_local_id, mark_entity_deleted},
    AppState,
};
use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::thread;
use tauri::{AppHandle, Manager, State};

const ENTITY_SCHEDULE_BLOCK: &str = "schedule_block";
const ENTITY_SCHEDULE_TEMPLATE: &str = "schedule_template";

fn trigger_shared_sync(app: &AppHandle, trigger: &'static str) {
    let sync_app = app.clone();
    thread::spawn(move || {
        let _ = crate::commands::sync::sync_object_storage_after_external_change(sync_app, trigger);
    });
    crate::commands::feishu::sync_feishu_bridge_after_local_change(app.clone(), trigger);
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleBlock {
    pub id: i64,
    pub schedule_date: String,
    pub title: String,
    pub note: Option<String>,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub source_today_item_id: Option<i64>,
    pub template_id: Option<i64>,
    pub start_minute: i64,
    pub end_minute: i64,
    pub status: String,
    pub linked_study_mode_id: Option<i64>,
    pub linked_focus_session_id: Option<i64>,
    pub has_conflict: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleTemplate {
    pub id: i64,
    pub title: String,
    pub note: Option<String>,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub weekdays: Vec<i64>,
    pub start_minute: i64,
    pub end_minute: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleTodayItem {
    pub id: i64,
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub subject_id: Option<i64>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleDay {
    pub date: String,
    pub weekday: i64,
    pub blocks: Vec<ScheduleBlock>,
    pub planned_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulePageData {
    pub selected_date: String,
    pub today_date: String,
    pub week_start_date: String,
    pub day_blocks: Vec<ScheduleBlock>,
    pub week_days: Vec<ScheduleDay>,
    pub today_items: Vec<ScheduleTodayItem>,
    pub templates: Vec<ScheduleTemplate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleBlockDraft {
    pub schedule_date: String,
    pub title: String,
    pub note: Option<String>,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub source_today_item_id: Option<i64>,
    pub start_minute: i64,
    pub end_minute: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleTemplateDraft {
    pub title: String,
    pub note: Option<String>,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub weekdays: Vec<i64>,
    pub start_minute: i64,
    pub end_minute: i64,
    pub enabled: bool,
}

#[tauri::command]
pub fn get_schedule_page_data(
    app: AppHandle,
    selected_date: Option<String>,
) -> Result<SchedulePageData, String> {
    let connection = open_database(&database_path(&app)?)?;
    let selected = parse_date_or_today(selected_date.as_deref())?;
    let week_start = week_start(selected);
    materialize_templates_for_range(&connection, week_start, week_start + Duration::days(6))?;
    load_schedule_page_data(&connection, selected)
}

#[tauri::command]
pub fn create_schedule_block(
    app: AppHandle,
    draft: ScheduleBlockDraft,
) -> Result<ScheduleBlock, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_block_draft(&connection, &draft)?;
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              start_minute, end_minute, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'planned', ?9, ?9)
            ",
            params![
                draft.schedule_date,
                draft.title.trim(),
                normalize_optional_string(draft.note),
                normalize_category_key(&draft.category_key),
                draft.subject_id,
                draft.source_today_item_id,
                draft.start_minute,
                draft.end_minute,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    let block = get_schedule_block_by_id(&connection, connection.last_insert_rowid())?;
    if let Some(source_today_item_id) = draft.source_today_item_id {
        ensure_sync_meta_for_local_id(
            &connection,
            ENTITY_SCHEDULE_BLOCK,
            block.id,
            Some(format!(
                "schedule_block:{}:source-today:{}:{}-{}",
                draft.schedule_date, source_today_item_id, draft.start_minute, draft.end_minute
            )),
            Utc::now().timestamp_millis(),
        )?;
    }
    trigger_shared_sync(&app, "schedule_change");
    Ok(block)
}

#[tauri::command]
pub fn create_schedule_block_from_today_item(
    app: AppHandle,
    today_item_id: i64,
    schedule_date: String,
    start_minute: i64,
    end_minute: i64,
) -> Result<ScheduleBlock, String> {
    let connection = open_database(&database_path(&app)?)?;
    let item = connection
        .query_row(
            "
            SELECT title, note, subject_id
            FROM today_plan_items
            WHERE id = ?1
            LIMIT 1
            ",
            params![today_item_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Today task not found.".to_string())?;

    create_schedule_block(
        app,
        ScheduleBlockDraft {
            schedule_date,
            title: item.0,
            note: item.1,
            category_key: category_key_for_subject_id(item.2).to_string(),
            subject_id: item.2,
            source_today_item_id: Some(today_item_id),
            start_minute,
            end_minute,
        },
    )
}

#[tauri::command]
pub fn update_schedule_block(
    app: AppHandle,
    id: i64,
    draft: ScheduleBlockDraft,
) -> Result<ScheduleBlock, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_block_draft(&connection, &draft)?;
    let now = Utc::now().to_rfc3339();
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
                start_minute = ?7,
                end_minute = ?8,
                updated_at = ?9
            WHERE id = ?10
            ",
            params![
                draft.schedule_date,
                draft.title.trim(),
                normalize_optional_string(draft.note),
                normalize_category_key(&draft.category_key),
                draft.subject_id,
                draft.source_today_item_id,
                draft.start_minute,
                draft.end_minute,
                now,
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    let block = get_schedule_block_by_id(&connection, id)?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(block)
}

#[tauri::command]
pub fn move_schedule_block(
    app: AppHandle,
    id: i64,
    schedule_date: String,
    start_minute: i64,
    end_minute: i64,
) -> Result<ScheduleBlock, String> {
    validate_schedule_date(&schedule_date)?;
    validate_time_range(start_minute, end_minute)?;
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE schedule_blocks
            SET schedule_date = ?1,
                start_minute = ?2,
                end_minute = ?3,
                updated_at = ?4
            WHERE id = ?5
            ",
            params![schedule_date, start_minute, end_minute, now, id],
        )
        .map_err(|error| error.to_string())?;

    let block = get_schedule_block_by_id(&connection, id)?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(block)
}

#[tauri::command]
pub fn delete_schedule_block(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    mark_entity_deleted(&connection, ENTITY_SCHEDULE_BLOCK, id, now)?;
    connection
        .execute("DELETE FROM schedule_blocks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(())
}

#[tauri::command]
pub fn create_schedule_template(
    app: AppHandle,
    draft: ScheduleTemplateDraft,
) -> Result<ScheduleTemplate, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_template_draft(&connection, &draft)?;
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            INSERT INTO schedule_templates (
              title, note, category_key, subject_id, weekdays, start_minute, end_minute,
              enabled, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            ",
            params![
                draft.title.trim(),
                normalize_optional_string(draft.note),
                normalize_category_key(&draft.category_key),
                draft.subject_id,
                weekdays_to_json(&draft.weekdays)?,
                draft.start_minute,
                draft.end_minute,
                if draft.enabled { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    let template = get_schedule_template_by_id(&connection, connection.last_insert_rowid())?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(template)
}

#[tauri::command]
pub fn update_schedule_template(
    app: AppHandle,
    id: i64,
    draft: ScheduleTemplateDraft,
) -> Result<ScheduleTemplate, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_template_draft(&connection, &draft)?;
    let now = Utc::now().to_rfc3339();
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
                updated_at = ?9
            WHERE id = ?10
            ",
            params![
                draft.title.trim(),
                normalize_optional_string(draft.note),
                normalize_category_key(&draft.category_key),
                draft.subject_id,
                weekdays_to_json(&draft.weekdays)?,
                draft.start_minute,
                draft.end_minute,
                if draft.enabled { 1 } else { 0 },
                now,
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    let template = get_schedule_template_by_id(&connection, id)?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(template)
}

#[tauri::command]
pub fn delete_schedule_template(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    mark_entity_deleted(&connection, ENTITY_SCHEDULE_TEMPLATE, id, now)?;
    connection
        .execute("DELETE FROM schedule_templates WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    trigger_shared_sync(&app, "schedule_change");
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn start_study_mode_from_schedule_block(
    app: AppHandle,
    state: State<'_, AppState>,
    block_id: i64,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    mode: String,
) -> Result<StudyModeState, String> {
    let mut connection = open_database(&database_path(&app)?)?;
    let next_state = start_study_mode_from_schedule_block_on_connection(
        &mut connection,
        state.inner(),
        block_id,
        planned_seconds,
        focus_seconds,
        break_seconds,
        long_break_seconds,
        long_break_interval,
        mode,
    )?;

    trigger_shared_sync(&app, "focus_state_change");
    trigger_shared_sync(&app, "schedule_change");
    focus::sync_focus_widget_for_state(&app, &next_state);
    Ok(next_state)
}

#[allow(clippy::too_many_arguments)]
fn start_study_mode_from_schedule_block_on_connection(
    connection: &mut Connection,
    state: &AppState,
    block_id: i64,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    mode: String,
) -> Result<StudyModeState, String> {
    let block = get_schedule_block_by_id(connection, block_id)?;
    let next_state = focus::start_study_mode_with_links_on_connection(
        connection,
        state,
        planned_seconds,
        focus_seconds,
        break_seconds,
        long_break_seconds,
        long_break_interval,
        mode,
        block.subject_id,
        StudyModeLinks {
            schedule_block_id: Some(block.id),
            today_plan_item_id: block.source_today_item_id,
        },
    )?;

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            UPDATE schedule_blocks
            SET status = 'running',
                linked_study_mode_id = ?1,
                linked_focus_session_id = ?2,
                updated_at = ?3
            WHERE id = ?4
            ",
            params![
                next_state.id,
                next_state
                    .current_session
                    .as_ref()
                    .map(|session| session.id),
                now,
                block_id
            ],
        )
        .map_err(|error| error.to_string())?;

    Ok(next_state)
}

pub(crate) fn mark_schedule_block_completed(
    connection: &Connection,
    block_id: Option<i64>,
    study_mode_id: i64,
    focus_session_id: Option<i64>,
    now: &str,
) -> Result<(), String> {
    let Some(block_id) = block_id else {
        return Ok(());
    };

    connection
        .execute(
            "
            UPDATE schedule_blocks
            SET status = 'completed',
                linked_study_mode_id = ?1,
                linked_focus_session_id = COALESCE(?2, linked_focus_session_id),
                updated_at = ?3
            WHERE id = ?4
            ",
            params![study_mode_id, focus_session_id, now, block_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_schedule_page_data(
    connection: &Connection,
    selected: NaiveDate,
) -> Result<SchedulePageData, String> {
    let selected_date = date_string(selected);
    let today_date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let week_start = week_start(selected);
    let mut week_days = Vec::new();
    for offset in 0..7 {
        let date = week_start + Duration::days(offset);
        let blocks = list_schedule_blocks(connection, &date_string(date))?;
        let planned_minutes = blocks
            .iter()
            .map(|block| block.end_minute.saturating_sub(block.start_minute))
            .sum();
        week_days.push(ScheduleDay {
            date: date_string(date),
            weekday: date.weekday().number_from_monday() as i64,
            blocks,
            planned_minutes,
        });
    }

    Ok(SchedulePageData {
        selected_date: selected_date.clone(),
        today_date,
        week_start_date: date_string(week_start),
        day_blocks: list_schedule_blocks(connection, &selected_date)?,
        week_days,
        today_items: list_today_items(connection, &selected_date)?,
        templates: list_schedule_templates(connection)?,
    })
}

fn materialize_templates_for_range(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(), String> {
    let templates = list_schedule_templates(connection)?;
    let existing_keys = existing_template_block_keys(connection, start, end)?;
    let mut date = start;
    while date <= end {
        let weekday = date.weekday().number_from_monday() as i64;
        let schedule_date = date_string(date);
        for template in templates.iter().filter(|template| template.enabled) {
            if !template.weekdays.contains(&weekday) {
                continue;
            }

            if existing_keys.contains(&(template.id, schedule_date.clone())) {
                continue;
            }

            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "
                    INSERT INTO schedule_blocks (
                      schedule_date, title, note, category_key, subject_id, template_id,
                      start_minute, end_minute, status, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'planned', ?9, ?9)
                    ",
                    params![
                        schedule_date,
                        template.title,
                        template.note,
                        template.category_key,
                        template.subject_id,
                        template.id,
                        template.start_minute,
                        template.end_minute,
                        now
                    ],
                )
                .map_err(|error| error.to_string())?;
            let local_id = connection.last_insert_rowid();
            ensure_sync_meta_for_local_id(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                local_id,
                Some(format!(
                    "schedule_block:{}:template:{}:{}-{}",
                    schedule_date, template.id, template.start_minute, template.end_minute
                )),
                Utc::now().timestamp_millis(),
            )?;
        }
        date += Duration::days(1);
    }
    Ok(())
}

fn existing_template_block_keys(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<HashSet<(i64, String)>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT template_id, schedule_date
            FROM schedule_blocks
            WHERE template_id IS NOT NULL
              AND schedule_date BETWEEN ?1 AND ?2
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![date_string(start), date_string(end)], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn list_schedule_blocks(
    connection: &Connection,
    schedule_date: &str,
) -> Result<Vec<ScheduleBlock>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, schedule_date, title, note, category_key, subject_id, source_today_item_id,
                   template_id, start_minute, end_minute, status, linked_study_mode_id,
                   linked_focus_session_id, created_at, updated_at
            FROM schedule_blocks
            WHERE schedule_date = ?1
            ORDER BY start_minute ASC, end_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let mut blocks = statement
        .query_map(params![schedule_date], row_to_schedule_block)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    mark_conflicts(&mut blocks);
    Ok(blocks)
}

fn list_schedule_templates(connection: &Connection) -> Result<Vec<ScheduleTemplate>, String> {
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
            let weekdays_raw: String = row.get(5)?;
            Ok(ScheduleTemplate {
                id: row.get(0)?,
                title: row.get(1)?,
                note: row.get(2)?,
                category_key: row.get(3)?,
                subject_id: row.get(4)?,
                weekdays: parse_weekdays(&weekdays_raw),
                start_minute: row.get(6)?,
                end_minute: row.get(7)?,
                enabled: row.get::<_, i64>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn list_today_items(
    connection: &Connection,
    today_date: &str,
) -> Result<Vec<ScheduleTodayItem>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, note, due_date, subject_id, completed
            FROM today_plan_items
            WHERE today_date = ?1
            ORDER BY completed ASC, sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![today_date], |row| {
            Ok(ScheduleTodayItem {
                id: row.get(0)?,
                title: row.get(1)?,
                note: row.get(2)?,
                due_date: row.get(3)?,
                subject_id: row.get(4)?,
                completed: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn get_schedule_block_by_id(connection: &Connection, id: i64) -> Result<ScheduleBlock, String> {
    connection
        .query_row(
            "
            SELECT id, schedule_date, title, note, category_key, subject_id, source_today_item_id,
                   template_id, start_minute, end_minute, status, linked_study_mode_id,
                   linked_focus_session_id, created_at, updated_at
            FROM schedule_blocks
            WHERE id = ?1
            ",
            params![id],
            row_to_schedule_block,
        )
        .map_err(|error| error.to_string())
}

fn get_schedule_template_by_id(
    connection: &Connection,
    id: i64,
) -> Result<ScheduleTemplate, String> {
    connection
        .query_row(
            "
            SELECT id, title, note, category_key, subject_id, weekdays, start_minute,
                   end_minute, enabled, created_at, updated_at
            FROM schedule_templates
            WHERE id = ?1
            ",
            params![id],
            |row| {
                let weekdays_raw: String = row.get(5)?;
                Ok(ScheduleTemplate {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    note: row.get(2)?,
                    category_key: row.get(3)?,
                    subject_id: row.get(4)?,
                    weekdays: parse_weekdays(&weekdays_raw),
                    start_minute: row.get(6)?,
                    end_minute: row.get(7)?,
                    enabled: row.get::<_, i64>(8)? != 0,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

fn row_to_schedule_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduleBlock> {
    Ok(ScheduleBlock {
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
        has_conflict: false,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn mark_conflicts(blocks: &mut [ScheduleBlock]) {
    for index in 0..blocks.len() {
        let has_conflict = blocks.iter().enumerate().any(|(other_index, other)| {
            other_index != index
                && blocks[index].start_minute < other.end_minute
                && other.start_minute < blocks[index].end_minute
        });
        blocks[index].has_conflict = has_conflict;
    }
}

fn validate_block_draft(connection: &Connection, draft: &ScheduleBlockDraft) -> Result<(), String> {
    validate_schedule_date(&draft.schedule_date)?;
    validate_time_range(draft.start_minute, draft.end_minute)?;
    validate_title(&draft.title)?;
    validate_category_key(&draft.category_key)?;
    validate_optional_subject_id(connection, draft.subject_id)?;
    if let Some(today_item_id) = draft.source_today_item_id {
        validate_today_item_id(connection, today_item_id)?;
    }
    Ok(())
}

fn validate_template_draft(
    connection: &Connection,
    draft: &ScheduleTemplateDraft,
) -> Result<(), String> {
    validate_title(&draft.title)?;
    validate_category_key(&draft.category_key)?;
    validate_time_range(draft.start_minute, draft.end_minute)?;
    validate_optional_subject_id(connection, draft.subject_id)?;
    if draft.weekdays.is_empty()
        || draft
            .weekdays
            .iter()
            .any(|weekday| !matches!(*weekday, 1..=7))
    {
        return Err("Weekdays must be between 1 and 7.".to_string());
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Title cannot be empty.".to_string());
    }
    Ok(())
}

fn validate_schedule_date(value: &str) -> Result<(), String> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| "Date must use YYYY-MM-DD.".to_string())
}

fn validate_time_range(start_minute: i64, end_minute: i64) -> Result<(), String> {
    if !(0..=1440).contains(&start_minute) || !(0..=1440).contains(&end_minute) {
        return Err("Time must be within the day.".to_string());
    }
    if end_minute <= start_minute {
        return Err("End time must be after start time.".to_string());
    }
    Ok(())
}

fn validate_category_key(value: &str) -> Result<(), String> {
    match normalize_category_key(value).as_str() {
        "politics" | "english" | "math" | "major" | "general" => Ok(()),
        _ => Err("Unknown schedule category.".to_string()),
    }
}

fn validate_optional_subject_id(
    connection: &Connection,
    subject_id: Option<i64>,
) -> Result<(), String> {
    if let Some(subject_id) = subject_id {
        let exists = connection
            .query_row(
                "SELECT 1 FROM subjects WHERE id = ?1",
                params![subject_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err("Subject does not exist.".to_string());
        }
    }
    Ok(())
}

fn validate_today_item_id(connection: &Connection, item_id: i64) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM today_plan_items WHERE id = ?1",
            params![item_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Err("Today task does not exist.".to_string());
    }
    Ok(())
}

fn parse_date_or_today(value: Option<&str>) -> Result<NaiveDate, String> {
    match value.map(str::trim).filter(|item| !item.is_empty()) {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| "Date must use YYYY-MM-DD.".to_string()),
        None => Ok(Local::now().date_naive()),
    }
}

fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

fn date_string(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_weekdays(raw: &str) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|weekday| matches!(*weekday, 1..=7))
        .collect()
}

fn weekdays_to_json(weekdays: &[i64]) -> Result<String, String> {
    let mut normalized = weekdays
        .iter()
        .copied()
        .filter(|weekday| matches!(*weekday, 1..=7))
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    serde_json::to_string(&normalized).map_err(|error| error.to_string())
}

fn normalize_category_key(value: &str) -> String {
    match value.trim() {
        "politics" | "english" | "math" | "major" | "general" => value.trim().to_string(),
        _ => "general".to_string(),
    }
}

fn category_key_for_subject_id(subject_id: Option<i64>) -> &'static str {
    match subject_id {
        Some(1) => "politics",
        Some(2) => "english",
        Some(3) => "math",
        Some(4) => "major",
        _ => "general",
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_database;
    use std::sync::Mutex;
    use tempfile::tempdir;

    fn test_state() -> AppState {
        AppState {
            active_session_id: Mutex::new(None),
            study_mode_active: Mutex::new(false),
            last_blocked_process: Mutex::new(None),
        }
    }

    #[test]
    fn starting_from_schedule_block_links_database_records() {
        let directory = tempdir().expect("create temp directory");
        let mut connection = open_database(&directory.path().join("schedule-start.sqlite3"))
            .expect("open test database");
        let now = Utc::now().to_rfc3339();

        connection
            .execute(
                "
            INSERT INTO today_plan_items (
              today_date, source_task_id, subject_id, title, note, due_date, sort_order,
              completed, synced_source_completion, created_at, updated_at
            ) VALUES ('2026-06-08', NULL, 2, 'Read English', NULL, NULL, 1, 0, 0, ?1, ?1)
            ",
                params![now],
            )
            .expect("insert today item");
        let today_item_id = connection.last_insert_rowid();

        connection.execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              template_id, start_minute, end_minute, status, created_at, updated_at
            ) VALUES ('2026-06-08', 'English block', NULL, 'english', 2, ?1, NULL, 540, 600, 'planned', ?2, ?2)
            ",
            params![today_item_id, now],
        ).expect("insert schedule block");
        let block_id = connection.last_insert_rowid();
        let state = test_state();

        let study_state = start_study_mode_from_schedule_block_on_connection(
            &mut connection,
            &state,
            block_id,
            7200,
            1500,
            300,
            900,
            4,
            "normal".to_string(),
        )
        .expect("start study mode from schedule block");

        assert_eq!(study_state.status, "active");
        assert_eq!(study_state.phase, "focus");
        assert_eq!(study_state.subject_id, Some(2));
        assert!(study_state.current_session.is_some());

        let (status, linked_study_mode_id, linked_focus_session_id): (
            String,
            Option<i64>,
            Option<i64>,
        ) = connection
            .query_row(
                "
            SELECT status, linked_study_mode_id, linked_focus_session_id
            FROM schedule_blocks
            WHERE id = ?1
            ",
                params![block_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load linked schedule block");
        assert_eq!(status, "running");
        assert_eq!(linked_study_mode_id, study_state.id);
        assert_eq!(
            linked_focus_session_id,
            study_state
                .current_session
                .as_ref()
                .map(|session| session.id),
        );

        let (schedule_block_id, today_plan_item_id, current_session_id): (
            Option<i64>,
            Option<i64>,
            Option<i64>,
        ) = connection
            .query_row(
                "
            SELECT schedule_block_id, today_plan_item_id, current_session_id
            FROM study_modes
            WHERE id = ?1
            ",
                params![study_state.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load study mode links");
        assert_eq!(schedule_block_id, Some(block_id));
        assert_eq!(today_plan_item_id, Some(today_item_id));
        assert_eq!(
            current_session_id,
            study_state
                .current_session
                .as_ref()
                .map(|session| session.id),
        );

        let (session_schedule_block_id, session_today_plan_item_id): (Option<i64>, Option<i64>) =
            connection
                .query_row(
                    "
            SELECT schedule_block_id, today_plan_item_id
            FROM focus_sessions
            WHERE id = ?1
            ",
                    params![study_state
                        .current_session
                        .as_ref()
                        .map(|session| session.id)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("load focus session links");
        assert_eq!(session_schedule_block_id, Some(block_id));
        assert_eq!(session_today_plan_item_id, Some(today_item_id));
    }
}

use crate::{storage::db::open_database, sync_package::mark_entity_deleted};
use chrono::{Local, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

const CATEGORY_NAME_SETTING_KEY: &str = "checklist_category_names";
const DEFAULT_CATEGORY_NAMES_JSON: &str =
    "{\"politics\":\"政治\",\"english\":\"英语\",\"math\":\"数学\",\"major\":\"专业课\",\"general\":\"通用\"}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ChecklistCategoryKey {
    Politics,
    English,
    Math,
    Major,
    General,
}

impl ChecklistCategoryKey {
    fn as_str(self) -> &'static str {
        match self {
            Self::Politics => "politics",
            Self::English => "english",
            Self::Math => "math",
            Self::Major => "major",
            Self::General => "general",
        }
    }

    fn default_label(self) -> &'static str {
        match self {
            Self::Politics => "政治",
            Self::English => "英语",
            Self::Math => "数学",
            Self::Major => "专业课",
            Self::General => "通用",
        }
    }
}

const CATEGORY_ORDER: [ChecklistCategoryKey; 5] = [
    ChecklistCategoryKey::Politics,
    ChecklistCategoryKey::English,
    ChecklistCategoryKey::Math,
    ChecklistCategoryKey::Major,
    ChecklistCategoryKey::General,
];

#[derive(Debug, Clone, Serialize)]
pub struct ChecklistTask {
    pub id: i64,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: i64,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayPlanItem {
    pub id: i64,
    pub today_date: String,
    pub source_task_id: Option<i64>,
    pub subject_id: Option<i64>,
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub sort_order: i64,
    pub completed: bool,
    pub synced_source_completion: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChecklistCategory {
    pub key: String,
    pub title: String,
    pub pending_tasks: Vec<ChecklistTask>,
    pub completed_tasks: Vec<ChecklistTask>,
    pub highlighted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChecklistPageData {
    pub today_date: String,
    pub active_category_key: String,
    pub highlighted_subject_id: Option<i64>,
    pub categories: Vec<ChecklistCategory>,
    pub today_items: Vec<TodayPlanItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistTaskDraft {
    pub category_key: String,
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayPlanItemDraft {
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub subject_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct TaskRecord {
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

#[tauri::command]
pub fn get_checklist_page_data(app: AppHandle) -> Result<ChecklistPageData, String> {
    let connection = open_database(&database_path(&app)?)?;
    let today_date = today_date_string();
    ensure_category_buckets(&connection)?;
    let highlighted_subject_id = get_active_study_subject_id(&connection)?;
    load_checklist_page_data(&connection, &today_date, highlighted_subject_id)
}

#[tauri::command]
pub fn create_checklist_task(
    app: AppHandle,
    draft: ChecklistTaskDraft,
) -> Result<ChecklistTask, String> {
    let connection = open_database(&database_path(&app)?)?;
    let category = parse_category_key(&draft.category_key)?;
    let board_scope = board_scope_for_category(category);
    ensure_category_bucket(&connection, &board_scope)?;

    let title = draft.title.trim();
    if title.is_empty() {
        return Err("任务标题不能为空".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let sort_order = next_sort_order(
        &connection,
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM checklist_tasks WHERE board_scope = ?1",
        params![board_scope.clone()],
    )?;

    connection
        .execute(
            "
            INSERT INTO checklist_tasks (
              board_scope,
              subject_id,
              column_id,
              title,
              note,
              due_date,
              sort_order,
              completed,
              created_at,
              updated_at
            ) VALUES (?1, NULL, (SELECT id FROM checklist_columns WHERE board_scope = ?1 ORDER BY sort_order ASC, id ASC LIMIT 1), ?2, ?3, ?4, ?5, 0, ?6, ?6)
            ",
            params![
                board_scope,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                sort_order,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    get_checklist_task_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn update_checklist_task(
    app: AppHandle,
    id: i64,
    draft: ChecklistTaskDraft,
) -> Result<ChecklistTask, String> {
    let connection = open_database(&database_path(&app)?)?;
    let category = parse_category_key(&draft.category_key)?;
    let board_scope = board_scope_for_category(category);
    ensure_category_bucket(&connection, &board_scope)?;

    let title = draft.title.trim();
    if title.is_empty() {
        return Err("任务标题不能为空".to_string());
    }

    connection
        .execute(
            "
            UPDATE checklist_tasks
            SET board_scope = ?1,
                subject_id = NULL,
                title = ?2,
                note = ?3,
                due_date = ?4,
                updated_at = ?5
            WHERE id = ?6
            ",
            params![
                board_scope,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    get_checklist_task_by_id(&connection, id)
}

#[tauri::command]
pub fn delete_checklist_task(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    mark_entity_deleted(&connection, "checklist_task", id, now)?;
    connection
        .execute(
            "DELETE FROM today_plan_items WHERE source_task_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute("DELETE FROM checklist_tasks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reorder_checklist_tasks(
    app: AppHandle,
    category_key: String,
    ordered_ids: Vec<i64>,
) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let category = parse_category_key(&category_key)?;
    let board_scope = board_scope_for_category(category);
    let now = Utc::now().to_rfc3339();

    for (index, id) in ordered_ids.iter().enumerate() {
        connection
            .execute(
                "
                UPDATE checklist_tasks
                SET sort_order = ?1,
                    updated_at = ?2
                WHERE id = ?3 AND board_scope = ?4
                ",
                params![index as i64, now, id, board_scope],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn complete_checklist_task(
    app: AppHandle,
    id: i64,
    completed: bool,
) -> Result<ChecklistTask, String> {
    let connection = open_database(&database_path(&app)?)?;
    connection
        .execute(
            "UPDATE checklist_tasks SET completed = ?1, updated_at = ?2 WHERE id = ?3",
            params![completed, Utc::now().to_rfc3339(), id],
        )
        .map_err(|error| error.to_string())?;

    get_checklist_task_by_id(&connection, id)
}

#[tauri::command]
pub fn add_task_to_today_plan(app: AppHandle, task_id: i64) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    let task = get_checklist_task_by_id(&connection, task_id)?;
    let today_date = today_date_string();

    let existing = connection
        .query_row(
            "
            SELECT id
            FROM today_plan_items
            WHERE today_date = ?1 AND source_task_id = ?2
            LIMIT 1
            ",
            params![today_date, task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(existing_id) = existing {
        return get_today_plan_item_by_id(&connection, existing_id);
    }

    let now = Utc::now().to_rfc3339();
    let sort_order = next_sort_order(
        &connection,
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM today_plan_items WHERE today_date = ?1",
        params![today_date.clone()],
    )?;

    connection
        .execute(
            "
            INSERT INTO today_plan_items (
              today_date,
              source_task_id,
              subject_id,
              title,
              note,
              due_date,
              sort_order,
              completed,
              synced_source_completion,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, ?8)
            ",
            params![
                today_date,
                task.id,
                task.subject_id,
                task.title,
                task.note,
                task.due_date,
                sort_order,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    get_today_plan_item_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn create_today_plan_item(
    app: AppHandle,
    draft: TodayPlanItemDraft,
) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_optional_subject_id(&connection, draft.subject_id)?;

    let title = draft.title.trim();
    if title.is_empty() {
        return Err("今日计划标题不能为空".to_string());
    }

    let today_date = today_date_string();
    let now = Utc::now().to_rfc3339();
    let sort_order = next_sort_order(
        &connection,
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM today_plan_items WHERE today_date = ?1",
        params![today_date.clone()],
    )?;

    connection
        .execute(
            "
            INSERT INTO today_plan_items (
              today_date,
              source_task_id,
              subject_id,
              title,
              note,
              due_date,
              sort_order,
              completed,
              synced_source_completion,
              created_at,
              updated_at
            ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?7)
            ",
            params![
                today_date,
                draft.subject_id,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                sort_order,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    get_today_plan_item_by_id(&connection, connection.last_insert_rowid())
}

#[tauri::command]
pub fn update_today_plan_item(
    app: AppHandle,
    id: i64,
    draft: TodayPlanItemDraft,
) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_optional_subject_id(&connection, draft.subject_id)?;

    let title = draft.title.trim();
    if title.is_empty() {
        return Err("今日计划标题不能为空".to_string());
    }

    connection
        .execute(
            "
            UPDATE today_plan_items
            SET subject_id = ?1,
                title = ?2,
                note = ?3,
                due_date = ?4,
                updated_at = ?5
            WHERE id = ?6
            ",
            params![
                draft.subject_id,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    get_today_plan_item_by_id(&connection, id)
}

#[tauri::command]
pub fn delete_today_plan_item(app: AppHandle, id: i64) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    mark_entity_deleted(&connection, "today_plan_item", id, now)?;
    connection
        .execute("DELETE FROM today_plan_items WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reorder_today_plan_items(app: AppHandle, ordered_ids: Vec<i64>) -> Result<(), String> {
    let connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();
    for (index, id) in ordered_ids.iter().enumerate() {
        connection
            .execute(
                "UPDATE today_plan_items SET sort_order = ?1, updated_at = ?2 WHERE id = ?3",
                params![index as i64, now, id],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn complete_today_plan_item(
    app: AppHandle,
    id: i64,
    completed: bool,
    sync_source_completion: bool,
) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    let item = get_today_plan_item_by_id(&connection, id)?;
    let now = Utc::now().to_rfc3339();

    connection
        .execute(
            "
            UPDATE today_plan_items
            SET completed = ?1,
                synced_source_completion = ?2,
                updated_at = ?3
            WHERE id = ?4
            ",
            params![completed, sync_source_completion, now, id],
        )
        .map_err(|error| error.to_string())?;

    if completed && sync_source_completion {
        if let Some(source_task_id) = item.source_task_id {
            connection
                .execute(
                    "
                    UPDATE checklist_tasks
                    SET completed = 1,
                        updated_at = ?1
                    WHERE id = ?2
                    ",
                    params![now, source_task_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    get_today_plan_item_by_id(&connection, id)
}

fn load_checklist_page_data(
    connection: &Connection,
    today_date: &str,
    highlighted_subject_id: Option<i64>,
) -> Result<ChecklistPageData, String> {
    ensure_category_buckets(connection)?;
    let category_names = load_category_names(connection)?;
    let all_tasks = list_all_checklist_tasks(connection)?;
    let today_items = list_today_plan_items(connection, today_date)?;

    let mut categories = Vec::new();
    for key in CATEGORY_ORDER {
        let category_key = key.as_str().to_string();
        let mut pending_tasks = Vec::new();
        let mut completed_tasks = Vec::new();

        for task in all_tasks
            .iter()
            .filter(|task| map_board_scope_to_category_key(&task.board_scope) == key)
        {
            let view_task = map_task_record_to_view(task, key);
            if task.completed {
                completed_tasks.push(view_task);
            } else {
                pending_tasks.push(view_task);
            }
        }

        categories.push(ChecklistCategory {
            key: category_key.clone(),
            title: category_names
                .get(key.as_str())
                .cloned()
                .unwrap_or_else(|| key.default_label().to_string()),
            pending_tasks,
            completed_tasks,
            highlighted: category_subject_id(key) == highlighted_subject_id,
        });
    }

    Ok(ChecklistPageData {
        today_date: today_date.to_string(),
        active_category_key: category_key_for_subject_id(highlighted_subject_id)
            .unwrap_or(ChecklistCategoryKey::Politics)
            .as_str()
            .to_string(),
        highlighted_subject_id,
        categories,
        today_items,
    })
}

fn get_active_study_subject_id(connection: &Connection) -> Result<Option<i64>, String> {
    connection
        .query_row(
            "
            SELECT subject_id
            FROM study_modes
            WHERE status = 'active'
            ORDER BY id DESC
            LIMIT 1
            ",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())
        .map(|value| value.flatten())
}

fn list_all_checklist_tasks(connection: &Connection) -> Result<Vec<TaskRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed, created_at, updated_at
            FROM checklist_tasks
            ORDER BY completed ASC, sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                board_scope: row.get(1)?,
                subject_id: row.get(2)?,
                title: row.get(3)?,
                note: row.get(4)?,
                due_date: row.get(5)?,
                sort_order: row.get(6)?,
                completed: row.get::<_, bool>(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn list_today_plan_items(
    connection: &Connection,
    today_date: &str,
) -> Result<Vec<TodayPlanItem>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            FROM today_plan_items
            WHERE today_date = ?1
            ORDER BY completed ASC, sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![today_date], row_to_today_plan_item)
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn map_task_record_to_view(task: &TaskRecord, key: ChecklistCategoryKey) -> ChecklistTask {
    ChecklistTask {
        id: task.id,
        category_key: key.as_str().to_string(),
        subject_id: task.subject_id.or(category_subject_id(key)),
        title: task.title.clone(),
        note: task.note.clone(),
        due_date: task.due_date.clone(),
        sort_order: task.sort_order,
        completed: task.completed,
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
    }
}

fn get_checklist_task_by_id(connection: &Connection, id: i64) -> Result<ChecklistTask, String> {
    let record = connection
        .query_row(
            "
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed, created_at, updated_at
            FROM checklist_tasks
            WHERE id = ?1
            ",
            params![id],
            |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    board_scope: row.get(1)?,
                    subject_id: row.get(2)?,
                    title: row.get(3)?,
                    note: row.get(4)?,
                    due_date: row.get(5)?,
                    sort_order: row.get(6)?,
                    completed: row.get::<_, bool>(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    let key = map_board_scope_to_category_key(&record.board_scope);
    Ok(map_task_record_to_view(&record, key))
}

fn get_today_plan_item_by_id(connection: &Connection, id: i64) -> Result<TodayPlanItem, String> {
    connection
        .query_row(
            "
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            FROM today_plan_items
            WHERE id = ?1
            ",
            params![id],
            row_to_today_plan_item,
        )
        .map_err(|error| error.to_string())
}

fn row_to_today_plan_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodayPlanItem> {
    Ok(TodayPlanItem {
        id: row.get(0)?,
        today_date: row.get(1)?,
        source_task_id: row.get(2)?,
        subject_id: row.get(3)?,
        title: row.get(4)?,
        note: row.get(5)?,
        due_date: row.get(6)?,
        sort_order: row.get(7)?,
        completed: row.get::<_, bool>(8)?,
        synced_source_completion: row.get::<_, bool>(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn ensure_category_buckets(connection: &Connection) -> Result<(), String> {
    for key in CATEGORY_ORDER {
        ensure_category_bucket(connection, board_scope_for_category(key).as_str())?;
    }
    Ok(())
}

fn ensure_category_bucket(connection: &Connection, board_scope: &str) -> Result<(), String> {
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

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            INSERT INTO checklist_columns (board_scope, name, sort_order, created_at, updated_at)
            VALUES (?1, '默认清单', 0, ?2, ?2)
            ",
            params![board_scope, now],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn load_category_names(connection: &Connection) -> Result<HashMap<String, String>, String> {
    let raw = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![CATEGORY_NAME_SETTING_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| DEFAULT_CATEGORY_NAMES_JSON.to_string());

    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| {
        serde_json::from_str(DEFAULT_CATEGORY_NAMES_JSON).unwrap_or(Value::Null)
    });

    let mut result = HashMap::new();
    for key in CATEGORY_ORDER {
        let value = parsed
            .get(key.as_str())
            .and_then(|item| item.as_str())
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .unwrap_or(key.default_label());
        result.insert(key.as_str().to_string(), value.to_string());
    }
    Ok(result)
}

fn map_board_scope_to_category_key(board_scope: &str) -> ChecklistCategoryKey {
    match board_scope {
        "checklist:politics" => ChecklistCategoryKey::Politics,
        "checklist:english" => ChecklistCategoryKey::English,
        "checklist:math" => ChecklistCategoryKey::Math,
        "checklist:major" => ChecklistCategoryKey::Major,
        "checklist:general" | "general" => ChecklistCategoryKey::General,
        value if value.starts_with("subject:") => {
            let subject_id = value
                .strip_prefix("subject:")
                .and_then(|item| item.parse::<i64>().ok())
                .unwrap_or_default();
            match subject_id {
                1 => ChecklistCategoryKey::Politics,
                2 => ChecklistCategoryKey::English,
                3 => ChecklistCategoryKey::Math,
                4 => ChecklistCategoryKey::Major,
                _ => ChecklistCategoryKey::General,
            }
        }
        _ => ChecklistCategoryKey::General,
    }
}

fn board_scope_for_category(key: ChecklistCategoryKey) -> String {
    match key {
        ChecklistCategoryKey::Politics => "checklist:politics".to_string(),
        ChecklistCategoryKey::English => "checklist:english".to_string(),
        ChecklistCategoryKey::Math => "checklist:math".to_string(),
        ChecklistCategoryKey::Major => "checklist:major".to_string(),
        ChecklistCategoryKey::General => "checklist:general".to_string(),
    }
}

fn category_subject_id(key: ChecklistCategoryKey) -> Option<i64> {
    match key {
        ChecklistCategoryKey::Politics => Some(1),
        ChecklistCategoryKey::English => Some(2),
        ChecklistCategoryKey::Math => Some(3),
        ChecklistCategoryKey::Major => Some(4),
        ChecklistCategoryKey::General => None,
    }
}

fn category_key_for_subject_id(subject_id: Option<i64>) -> Option<ChecklistCategoryKey> {
    match subject_id {
        Some(1) => Some(ChecklistCategoryKey::Politics),
        Some(2) => Some(ChecklistCategoryKey::English),
        Some(3) => Some(ChecklistCategoryKey::Math),
        Some(4) => Some(ChecklistCategoryKey::Major),
        _ => None,
    }
}

fn parse_category_key(value: &str) -> Result<ChecklistCategoryKey, String> {
    match value.trim() {
        "politics" => Ok(ChecklistCategoryKey::Politics),
        "english" => Ok(ChecklistCategoryKey::English),
        "math" => Ok(ChecklistCategoryKey::Math),
        "major" => Ok(ChecklistCategoryKey::Major),
        "general" => Ok(ChecklistCategoryKey::General),
        _ => Err("未知的清单分类".to_string()),
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
            return Err("科目不存在".to_string());
        }
    }
    Ok(())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn today_date_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn next_sort_order<P>(connection: &Connection, sql: &str, params: P) -> Result<i64, String>
where
    P: rusqlite::Params,
{
    connection
        .query_row(sql, params, |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

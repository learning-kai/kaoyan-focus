use crate::{
    commands::schedule::cascade_schedule_blocks_for_today_item_completion,
    storage::db::open_database,
    sync_package::{ensure_sync_meta_for_local_id, mark_entity_deleted},
};
use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::thread;
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
    pub priority: String,
    pub estimated_minutes: i64,
    pub ai_pinned: bool,
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
    pub priority: String,
    pub estimated_minutes: i64,
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
    // AI 排期属性。缺省时保持既有行为：priority = medium、estimated_minutes = 0（未估）。
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub ai_pinned: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayPlanItemDraft {
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub subject_id: Option<i64>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
}

/// 规范化优先级取值。非法值一律回落为 medium，避免脏数据进入排期计算。
fn normalize_priority(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some("high") => "high".to_string(),
        Some("low") => "low".to_string(),
        _ => "medium".to_string(),
    }
}

/// 规范化预计耗时：负数与非数字回落为 0（0 表示未估时，排期时取 default_block_minutes）。
fn normalize_estimated_minutes(value: Option<i64>) -> i64 {
    value.filter(|minutes| *minutes > 0).unwrap_or(0)
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
    priority: String,
    estimated_minutes: i64,
    ai_pinned: bool,
    created_at: String,
    updated_at: String,
}

fn trigger_shared_sync(app: &AppHandle, trigger: &'static str) {
    let sync_app = app.clone();
    thread::spawn(move || {
        let _ = crate::commands::sync::sync_object_storage_after_external_change(sync_app, trigger);
    });
    crate::commands::feishu::sync_feishu_bridge_after_local_change(app.clone(), trigger);
}

#[tauri::command]
pub fn get_checklist_page_data(
    app: AppHandle,
    selected_date: Option<String>,
) -> Result<ChecklistPageData, String> {
    let connection = open_database(&database_path(&app)?)?;
    let today_date = plan_date_string(selected_date)?;
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
              priority,
              estimated_minutes,
              ai_pinned,
              created_at,
              updated_at
            ) VALUES (?1, NULL, (SELECT id FROM checklist_columns WHERE board_scope = ?1 ORDER BY sort_order ASC, id ASC LIMIT 1), ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?9)
            ",
            params![
                board_scope,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                sort_order,
                normalize_priority(draft.priority),
                normalize_estimated_minutes(draft.estimated_minutes),
                if draft.ai_pinned.unwrap_or(false) { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    let task = get_checklist_task_by_id(&connection, connection.last_insert_rowid())?;
    trigger_shared_sync(&app, "checklist_change");
    Ok(task)
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
                priority = ?5,
                estimated_minutes = ?6,
                ai_pinned = COALESCE(?7, ai_pinned),
                updated_at = ?8
            WHERE id = ?9
            ",
            params![
                board_scope,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                normalize_priority(draft.priority),
                normalize_estimated_minutes(draft.estimated_minutes),
                draft.ai_pinned.map(|value| if value { 1 } else { 0 }),
                Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    let task = get_checklist_task_by_id(&connection, id)?;
    trigger_shared_sync(&app, "checklist_change");
    Ok(task)
}

#[tauri::command]
pub fn delete_checklist_task(app: AppHandle, id: i64) -> Result<(), String> {
    let mut connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        mark_entity_deleted(&transaction, "checklist_task", id, now)?;
        transaction
            .execute(
                "DELETE FROM today_plan_items WHERE source_task_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute("DELETE FROM checklist_tasks WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    trigger_shared_sync(&app, "checklist_change");
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

    trigger_shared_sync(&app, "checklist_change");
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

    let task = get_checklist_task_by_id(&connection, id)?;
    trigger_shared_sync(&app, "checklist_change");
    Ok(task)
}

#[tauri::command]
pub fn add_task_to_today_plan(
    app: AppHandle,
    task_id: i64,
    selected_date: Option<String>,
) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    // 先校验任务存在，保证「任务不存在」的错误早于任何写入与日期解析。
    let _ = get_checklist_task_by_id(&connection, task_id)?;
    let today_date = plan_date_string(selected_date)?;

    let (item_id, created) = ensure_today_plan_item_for_task(&connection, task_id, &today_date)?;
    let item = get_today_plan_item_by_id(&connection, item_id)?;

    // 已存在于今日计划时不重复补 sync_meta、也不触发同步（与抽出前的行为一致）。
    if created {
        ensure_sync_meta_for_local_id(
            &connection,
            "today_plan_item",
            item.id,
            Some(format!(
                "today_plan:{}:source-task:{}",
                item.today_date, task_id
            )),
            Utc::now().timestamp_millis(),
        )?;
        trigger_shared_sync(&app, "today_plan_change");
    }

    Ok(item)
}

#[tauri::command]
pub fn create_today_plan_item(
    app: AppHandle,
    draft: TodayPlanItemDraft,
    selected_date: Option<String>,
) -> Result<TodayPlanItem, String> {
    let connection = open_database(&database_path(&app)?)?;
    validate_optional_subject_id(&connection, draft.subject_id)?;

    let title = draft.title.trim();
    if title.is_empty() {
        return Err("计划任务标题不能为空".to_string());
    }

    let today_date = plan_date_string(selected_date)?;
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
              priority,
              estimated_minutes,
              created_at,
              updated_at
            ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7, ?8, ?9, ?9)
            ",
            params![
                today_date,
                draft.subject_id,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                sort_order,
                normalize_priority(draft.priority),
                normalize_estimated_minutes(draft.estimated_minutes),
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    let item = get_today_plan_item_by_id(&connection, connection.last_insert_rowid())?;
    trigger_shared_sync(&app, "today_plan_change");
    Ok(item)
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
                priority = ?5,
                estimated_minutes = ?6,
                updated_at = ?7
            WHERE id = ?8
            ",
            params![
                draft.subject_id,
                title,
                normalize_optional_string(draft.note),
                normalize_optional_string(draft.due_date),
                normalize_priority(draft.priority),
                normalize_estimated_minutes(draft.estimated_minutes),
                Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|error| error.to_string())?;

    let item = get_today_plan_item_by_id(&connection, id)?;
    trigger_shared_sync(&app, "today_plan_change");
    Ok(item)
}

#[tauri::command]
pub fn delete_today_plan_item(app: AppHandle, id: i64) -> Result<(), String> {
    let mut connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().timestamp_millis();
    {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        mark_entity_deleted(&transaction, "today_plan_item", id, now)?;
        transaction
            .execute("DELETE FROM today_plan_items WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    trigger_shared_sync(&app, "today_plan_change");
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
    trigger_shared_sync(&app, "today_plan_change");
    Ok(())
}

#[tauri::command]
pub fn complete_today_plan_item(
    app: AppHandle,
    id: i64,
    completed: bool,
    sync_source_completion: bool,
) -> Result<TodayPlanItem, String> {
    let mut connection = open_database(&database_path(&app)?)?;
    let now = Utc::now().to_rfc3339();

    let item = {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let item = get_today_plan_item_by_id(&transaction, id)?;

        transaction
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
                transaction
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

        cascade_schedule_blocks_for_today_item_completion(&transaction, id, completed, &now)?;

        let item = get_today_plan_item_by_id(&transaction, id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        item
    };
    trigger_shared_sync(&app, "today_plan_change");
    Ok(item)
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
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed,
                   priority, estimated_minutes, ai_pinned, created_at, updated_at
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
                priority: row.get(8)?,
                estimated_minutes: row.get(9)?,
                ai_pinned: row.get::<_, bool>(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

// ── 供 AI 智能日程规划使用的只读接口 ──
//
// 刻意不暴露私有的 `TaskRecord` / `ChecklistCategoryKey`，避免把清单内部实现细节
// 泄漏进 `commands::ai_scheduler`。分类键已在返回前归一化为五值枚举的字符串形式。

/// 参与自动排期的**队列条目**（`today_plan_items` 的一行）。
///
/// 排期单位是「今日 / 计划队列」里的条目，而不是整个清单的任务：用户把任务加进队列
/// 表达的是「今天要做这些」，那才是排期的输入。队列里手动新建的临时条目
/// （`source_task_id = None`）同样是队列成员，必须一起排。
///
/// **不再按 `ai_pinned` 过滤**：既然「在队列里」本身就是用户的显式选择，再叠一层
/// 用户看不见也改不了的隐藏过滤只会让「加进今天却没被排期」无法解释
/// （`ai_pinned` 目前也没有 UI 入口）。这是对方案 §4.2 第 1 步的有意修正。
#[derive(Debug, Clone)]
pub(crate) struct SchedulableQueueItem {
    /// `today_plan_items.id` —— 本次排期的唯一标识。
    pub item_id: i64,
    /// 清单来源；手动新建的临时条目为 `None`。
    pub source_task_id: Option<i64>,
    pub category_key: String,
    pub subject_id: Option<i64>,
    pub title: String,
    pub note: Option<String>,
    pub due_date: Option<String>,
    pub priority: String,
    /// 0 表示未估时。
    pub estimated_minutes: i64,
}

/// 指定日期的可排期队列条目：该日队列里未完成的条目。
///
/// `board_scope` 靠 LEFT JOIN 取到——它是分类的**权威来源**（与
/// `map_task_record_to_view` 一致）；来源任务已被删除的手动条目则回落到 `subject_id`
/// 反查分类，再不行算「通用」。
pub(crate) fn list_queue_items(
    connection: &Connection,
    today_date: &str,
) -> Result<Vec<SchedulableQueueItem>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT queue.id, queue.source_task_id, queue.subject_id, queue.title, queue.note,
                   queue.due_date, queue.priority, queue.estimated_minutes, task.board_scope
            FROM today_plan_items AS queue
            LEFT JOIN checklist_tasks AS task ON task.id = queue.source_task_id
            WHERE queue.today_date = ?1 AND queue.completed = 0
            ORDER BY queue.sort_order ASC, queue.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![today_date], |row| {
            let source_task_id: Option<i64> = row.get(1)?;
            let subject_id: Option<i64> = row.get(2)?;
            let board_scope: Option<String> = row.get(8)?;
            let key = board_scope
                .as_deref()
                .map(map_board_scope_to_category_key)
                .or_else(|| category_key_for_subject_id(subject_id))
                .unwrap_or(ChecklistCategoryKey::General);
            Ok(SchedulableQueueItem {
                item_id: row.get(0)?,
                source_task_id,
                category_key: key.as_str().to_string(),
                // 与 `map_task_record_to_view` 保持一致：未显式关联科目时用分类固有科目。
                subject_id: subject_id.or(category_subject_id(key)),
                title: row.get(3)?,
                note: row.get(4)?,
                due_date: row.get(5)?,
                priority: row.get(6)?,
                estimated_minutes: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// 分类键 → 显示名，直接复用设置页维护的 `checklist_category_names`。
pub(crate) fn category_label_map(
    connection: &Connection,
) -> Result<HashMap<String, String>, String> {
    load_category_names(connection)
}

/// apply 阶段漂移检测所需的队列条目当前状态。
#[derive(Debug, Clone)]
pub(crate) struct QueueItemDriftState {
    pub completed: bool,
    pub priority: String,
    pub due_date: Option<String>,
    pub estimated_minutes: i64,
}

/// 读取队列条目当前状态；`Ok(None)` 表示条目已被移出队列或删除。
///
/// 漂移检测必须落在**队列条目**上（而不是它的来源任务）：草案是照队列生成的，
/// 用户把条目移出队列、勾选完成、或改了优先级，都会让草案失去依据。
pub(crate) fn queue_item_drift_state(
    connection: &Connection,
    item_id: i64,
) -> Result<Option<QueueItemDriftState>, String> {
    connection
        .query_row(
            "SELECT completed, priority, due_date, estimated_minutes FROM today_plan_items WHERE id = ?1",
            params![item_id],
            |row| {
                Ok(QueueItemDriftState {
                    completed: row.get::<_, bool>(0)?,
                    priority: row.get(1)?,
                    due_date: row.get(2)?,
                    estimated_minutes: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

/// 在给定连接上把清单任务补进今日计划，返回 `(item_id, 是否新建)`。
///
/// 与命令 `add_task_to_today_plan` 的区别：**不打开新连接、不触发同步**，因此可以在
/// `apply` 的事务内安全调用（方案 §8.2 要求抽成可复用函数）。原本该命令自带一套
/// 连接与同步逻辑，若 apply 复用它会在事务中另开连接，导致写锁冲突。
pub(crate) fn ensure_today_plan_item_for_task(
    connection: &Connection,
    task_id: i64,
    today_date: &str,
) -> Result<(i64, bool), String> {
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
        return Ok((existing_id, false));
    }

    let task = get_checklist_task_by_id(connection, task_id)?;
    let now = Utc::now().to_rfc3339();
    let sort_order = next_sort_order(
        connection,
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM today_plan_items WHERE today_date = ?1",
        params![today_date],
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
              priority,
              estimated_minutes,
              created_at,
              updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, ?9, ?10, ?10)
            ",
            params![
                today_date,
                task.id,
                task.subject_id,
                task.title,
                task.note,
                task.due_date,
                sort_order,
                // 从来源任务继承 AI 排期属性，避免加入今日计划后丢失。
                task.priority.clone(),
                task.estimated_minutes,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    Ok((connection.last_insert_rowid(), true))
}

fn list_today_plan_items(
    connection: &Connection,
    today_date: &str,
) -> Result<Vec<TodayPlanItem>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, priority, estimated_minutes, created_at, updated_at
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
        priority: task.priority.clone(),
        estimated_minutes: task.estimated_minutes,
        ai_pinned: task.ai_pinned,
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
    }
}

fn get_checklist_task_by_id(connection: &Connection, id: i64) -> Result<ChecklistTask, String> {
    let record = connection
        .query_row(
            "
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed,
                   priority, estimated_minutes, ai_pinned, created_at, updated_at
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
                    priority: row.get(8)?,
                    estimated_minutes: row.get(9)?,
                    ai_pinned: row.get::<_, bool>(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
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
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, priority, estimated_minutes, created_at, updated_at
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
        priority: row.get(10)?,
        estimated_minutes: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
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

fn plan_date_string(selected_date: Option<String>) -> Result<String, String> {
    let Some(selected_date) = selected_date else {
        return Ok(today_date_string());
    };

    let value = selected_date.trim();
    if value.is_empty() {
        return Ok(today_date_string());
    }

    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%Y-%m-%d").to_string())
        .map_err(|_| "计划日期必须使用 YYYY-MM-DD 格式".to_string())
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

// 部分辅助函数有意位于测试模块之后；clippy 新版本默认告警 items_after_test_module，此处显式允许以保持仓库结构。
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::plan_date_string;
    use super::{list_queue_items, SchedulableQueueItem};
    use crate::storage::db::open_database;
    use rusqlite::{params, Connection};
    use std::path::PathBuf;

    #[test]
    fn selected_plan_date_accepts_a_future_calendar_date() {
        assert_eq!(
            plan_date_string(Some("2026-08-05".to_string())),
            Ok("2026-08-05".to_string())
        );
    }

    #[test]
    fn selected_plan_date_rejects_invalid_calendar_date() {
        assert!(plan_date_string(Some("2026-02-30".to_string())).is_err());
    }

    fn temp_database(name: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(name);
        (directory, path)
    }

    /// 手动队列条目夹具：无清单来源、无科目。
    fn seed_queue_row(connection: &Connection, id: i64, today_date: &str, completed: bool) {
        connection
            .execute(
                "
                INSERT INTO today_plan_items (
                  id, today_date, source_task_id, subject_id, title, note, due_date, sort_order,
                  completed, synced_source_completion, priority, estimated_minutes,
                  created_at, updated_at
                ) VALUES (?1, ?2, NULL, NULL, ?3, NULL, NULL, 0, ?4, 0, 'medium', 60, 'now', 'now')
                ",
                params![id, today_date, format!("条目{id}"), i64::from(completed)],
            )
            .expect("seed queue row");
    }

    /// 排期输入必须是「指定日期、未完成」的队列条目 —— 这是 2026-09-26 修正的核心：
    /// 旧实现排的是整个清单（`completed = 0 AND ai_pinned = 0`），用户无法用队列
    /// 控制「这次排什么」。
    #[test]
    fn queue_items_are_scoped_to_one_date_and_unfinished_only() {
        let (_directory, path) = temp_database("checklist-queue.sqlite3");
        let connection = open_database(&path).expect("open db");

        seed_queue_row(&connection, 1, "2026-09-25", false);
        seed_queue_row(&connection, 2, "2026-09-25", true);
        seed_queue_row(&connection, 3, "2026-09-26", false);

        let items: Vec<SchedulableQueueItem> =
            list_queue_items(&connection, "2026-09-25").expect("list");

        assert_eq!(
            items.iter().map(|item| item.item_id).collect::<Vec<_>>(),
            vec![1],
            "只取当天、未完成的队列条目"
        );
        // 无清单来源、无科目的手动条目 → 分类回落为「通用」。
        assert_eq!(items[0].category_key, "general");
        assert_eq!(items[0].source_task_id, None);
    }
}

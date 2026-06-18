use rusqlite::{params, Connection};
use std::{fs, path::Path};

const DEFAULT_SUBJECTS: [(&str, &str, &str, &str); 4] = [
    ("subject-1", "subject-politics", "政治", "#ef4444"),
    ("subject-2", "subject-english", "英语", "#3b82f6"),
    ("subject-3", "subject-math", "数学", "#16a34a"),
    ("subject-4", "subject-major", "专业课", "#a855f7"),
];

pub fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    configure_connection(&connection)?;
    run_migrations(&connection)?;
    seed_default_subjects(&connection)?;
    normalize_default_subjects(&connection)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection) -> Result<(), String> {
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            ",
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn run_migrations(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS focus_sessions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              mode TEXT NOT NULL DEFAULT 'normal',
              subject_id INTEGER,
              planned_seconds INTEGER NOT NULL,
              actual_seconds INTEGER NOT NULL DEFAULT 0,
              paused_seconds INTEGER NOT NULL DEFAULT 0,
              followed_by_break_type TEXT,
              schedule_block_id INTEGER,
              today_plan_item_id INTEGER,
              started_at TEXT NOT NULL,
              ended_at TEXT,
              status TEXT NOT NULL,
              end_reason TEXT,
              interruption_count INTEGER NOT NULL DEFAULT 0,
              emergency_exit_count INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS study_modes (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              mode TEXT NOT NULL DEFAULT 'normal',
              subject_id INTEGER,
              planned_seconds INTEGER NOT NULL,
              focus_seconds INTEGER NOT NULL,
              break_seconds INTEGER NOT NULL,
              long_break_seconds INTEGER NOT NULL DEFAULT 900,
              long_break_interval INTEGER NOT NULL DEFAULT 4,
              whitelist_enabled INTEGER NOT NULL DEFAULT 1,
              phase TEXT NOT NULL,
              cycle_index INTEGER NOT NULL DEFAULT 1,
              started_at TEXT NOT NULL,
              phase_started_at TEXT NOT NULL,
              paused_at TEXT,
              accumulated_study_seconds INTEGER NOT NULL DEFAULT 0,
              total_paused_seconds INTEGER NOT NULL DEFAULT 0,
              phase_paused_seconds INTEGER NOT NULL DEFAULT 0,
              paused_stage_elapsed_seconds INTEGER NOT NULL DEFAULT 0,
              state_revision INTEGER NOT NULL DEFAULT 1,
              ended_at TEXT,
              current_session_id INTEGER,
              schedule_block_id INTEGER,
              today_plan_item_id INTEGER,
              last_control_device_id TEXT,
              last_control_action TEXT,
              last_control_at INTEGER,
              status TEXT NOT NULL DEFAULT 'active',
              finish_reason TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY (current_session_id) REFERENCES focus_sessions(id)
            );

            CREATE TABLE IF NOT EXISTS whitelist_apps (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL,
              process_name TEXT NOT NULL,
              path TEXT,
              match_type TEXT NOT NULL DEFAULT 'process_name',
              subject_id INTEGER,
              note TEXT,
              enabled INTEGER NOT NULL DEFAULT 1,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_events (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id INTEGER NOT NULL,
              process_name TEXT NOT NULL,
              process_path TEXT,
              window_title TEXT,
              event_type TEXT NOT NULL,
              action_taken TEXT,
              created_at TEXT NOT NULL,
              FOREIGN KEY (session_id) REFERENCES focus_sessions(id)
            );

            CREATE TABLE IF NOT EXISTS subjects (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL,
              color TEXT,
              enabled INTEGER NOT NULL DEFAULT 1,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_meta (
              entity_type TEXT NOT NULL,
              local_id INTEGER NOT NULL,
              sync_id TEXT NOT NULL,
              deleted_at INTEGER,
              created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (entity_type, local_id)
            );

            CREATE TABLE IF NOT EXISTS checklist_columns (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              board_scope TEXT NOT NULL,
              name TEXT NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS checklist_tasks (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              board_scope TEXT NOT NULL,
              subject_id INTEGER,
              column_id INTEGER NOT NULL,
              title TEXT NOT NULL,
              note TEXT,
              due_date TEXT,
              sort_order INTEGER NOT NULL DEFAULT 0,
              completed INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY (column_id) REFERENCES checklist_columns(id)
            );

            CREATE TABLE IF NOT EXISTS today_plan_items (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              today_date TEXT NOT NULL,
              source_task_id INTEGER,
              subject_id INTEGER,
              title TEXT NOT NULL,
              note TEXT,
              due_date TEXT,
              sort_order INTEGER NOT NULL DEFAULT 0,
              completed INTEGER NOT NULL DEFAULT 0,
              synced_source_completion INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY (source_task_id) REFERENCES checklist_tasks(id)
            );

            CREATE TABLE IF NOT EXISTS schedule_templates (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              title TEXT NOT NULL,
              note TEXT,
              category_key TEXT NOT NULL DEFAULT 'general',
              subject_id INTEGER,
              weekdays TEXT NOT NULL DEFAULT '[]',
              start_minute INTEGER NOT NULL,
              end_minute INTEGER NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schedule_blocks (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              schedule_date TEXT NOT NULL,
              title TEXT NOT NULL,
              note TEXT,
              category_key TEXT NOT NULL DEFAULT 'general',
              subject_id INTEGER,
              source_today_item_id INTEGER,
              template_id INTEGER,
              start_minute INTEGER NOT NULL,
              end_minute INTEGER NOT NULL,
              status TEXT NOT NULL DEFAULT 'planned',
              linked_study_mode_id INTEGER,
              linked_focus_session_id INTEGER,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY (source_today_item_id) REFERENCES today_plan_items(id),
              FOREIGN KEY (template_id) REFERENCES schedule_templates(id),
              FOREIGN KEY (linked_study_mode_id) REFERENCES study_modes(id),
              FOREIGN KEY (linked_focus_session_id) REFERENCES focus_sessions(id)
            );

            CREATE TABLE IF NOT EXISTS alarms (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              title TEXT NOT NULL,
              note TEXT,
              alarm_date TEXT NOT NULL,
              alarm_time TEXT NOT NULL,
              alarm_at TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              status TEXT NOT NULL DEFAULT 'scheduled',
              fired_at TEXT,
              dismissed_at TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS daily_reviews (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              review_date TEXT NOT NULL UNIQUE,
              summary TEXT,
              blockers TEXT,
              tomorrow_focus TEXT,
              mood_score INTEGER NOT NULL DEFAULT 3,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS weekly_reviews (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              week_start_date TEXT NOT NULL UNIQUE,
              summary TEXT,
              blockers TEXT,
              next_week_focus TEXT,
              mood_score INTEGER NOT NULL DEFAULT 3,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS email_notification_logs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              entity_type TEXT NOT NULL,
              entity_id INTEGER NOT NULL,
              due_date TEXT NOT NULL,
              reminder_type TEXT NOT NULL DEFAULT 'due_tomorrow_21',
              sent_at TEXT NOT NULL,
              recipient TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS feishu_sync_links (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              entity_type TEXT NOT NULL,
              local_id INTEGER,
              local_sync_id TEXT NOT NULL,
              remote_kind TEXT NOT NULL,
              remote_id TEXT NOT NULL,
              remote_parent_id TEXT,
              remote_etag TEXT,
              remote_change_key TEXT,
              remote_last_modified TEXT,
              last_synced_at TEXT NOT NULL,
              deleted_at TEXT,
              UNIQUE(entity_type, local_sync_id, remote_kind),
              UNIQUE(remote_kind, remote_id)
            );

            CREATE TABLE IF NOT EXISTS calendar_sync_links (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              entity_type TEXT NOT NULL,
              local_id INTEGER,
              local_sync_id TEXT NOT NULL,
              provider TEXT NOT NULL,
              remote_id TEXT NOT NULL,
              remote_parent_id TEXT,
              remote_etag TEXT,
              remote_fingerprint TEXT,
              remote_last_modified TEXT,
              last_synced_at TEXT NOT NULL,
              deleted_at TEXT,
              UNIQUE(entity_type, local_sync_id, provider),
              UNIQUE(provider, remote_id)
            );

            CREATE TABLE IF NOT EXISTS feishu_sync_runs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              run_id TEXT NOT NULL,
              trigger TEXT NOT NULL,
              status TEXT NOT NULL,
              started_at TEXT NOT NULL,
              finished_at TEXT NOT NULL,
              duration_ms INTEGER NOT NULL DEFAULT 0,
              pushed_count INTEGER NOT NULL DEFAULT 0,
              pulled_count INTEGER NOT NULL DEFAULT 0,
              deleted_count INTEGER NOT NULL DEFAULT 0,
              conflict_count INTEGER NOT NULL DEFAULT 0,
              task_count INTEGER NOT NULL DEFAULT 0,
              calendar_count INTEGER NOT NULL DEFAULT 0,
              message TEXT NOT NULL,
              error_message TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_checklist_columns_scope_sort
              ON checklist_columns (board_scope, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_checklist_tasks_column_sort
              ON checklist_tasks (column_id, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_checklist_tasks_scope_sort
              ON checklist_tasks (board_scope, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_today_plan_items_date_sort
              ON today_plan_items (today_date, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_schedule_blocks_date_start
              ON schedule_blocks (schedule_date, start_minute, id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_schedule_blocks_template_date
              ON schedule_blocks (template_id, schedule_date)
              WHERE template_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_schedule_templates_enabled
              ON schedule_templates (enabled, start_minute, id);
            CREATE INDEX IF NOT EXISTS idx_alarms_due
              ON alarms (enabled, status, alarm_at, id);
            CREATE INDEX IF NOT EXISTS idx_alarms_status
              ON alarms (status, alarm_at, id);
            CREATE INDEX IF NOT EXISTS idx_daily_reviews_date
              ON daily_reviews (review_date);
            CREATE INDEX IF NOT EXISTS idx_weekly_reviews_week_start
              ON weekly_reviews (week_start_date);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_email_notification_logs_unique
              ON email_notification_logs (entity_type, entity_id, due_date, reminder_type);
            CREATE INDEX IF NOT EXISTS idx_feishu_sync_links_local
              ON feishu_sync_links (entity_type, local_id);
            CREATE INDEX IF NOT EXISTS idx_feishu_sync_links_sync_id
              ON feishu_sync_links (local_sync_id);
            CREATE INDEX IF NOT EXISTS idx_calendar_sync_links_local
              ON calendar_sync_links (entity_type, local_id);
            CREATE INDEX IF NOT EXISTS idx_calendar_sync_links_sync_id
              ON calendar_sync_links (local_sync_id);
            CREATE INDEX IF NOT EXISTS idx_calendar_sync_links_provider_remote
              ON calendar_sync_links (provider, remote_id);
            CREATE INDEX IF NOT EXISTS idx_feishu_sync_runs_finished
              ON feishu_sync_runs (finished_at DESC);
            ",
        )
        .map_err(|error| error.to_string())?;

    add_column_if_missing(
        connection,
        "study_modes",
        "long_break_seconds",
        "INTEGER NOT NULL DEFAULT 900",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "long_break_interval",
        "INTEGER NOT NULL DEFAULT 4",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "whitelist_enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(connection, "study_modes", "paused_at", "TEXT")?;
    add_column_if_missing(
        connection,
        "study_modes",
        "accumulated_study_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "total_paused_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "phase_paused_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "paused_stage_elapsed_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "study_modes",
        "state_revision",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(connection, "study_modes", "last_control_device_id", "TEXT")?;
    add_column_if_missing(connection, "study_modes", "last_control_action", "TEXT")?;
    add_column_if_missing(connection, "study_modes", "last_control_at", "INTEGER")?;
    backfill_study_mode_accumulated_seconds(connection)?;
    add_column_if_missing(
        connection,
        "focus_sessions",
        "paused_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "focus_sessions",
        "followed_by_break_type",
        "TEXT",
    )?;
    add_column_if_missing(connection, "focus_sessions", "schedule_block_id", "INTEGER")?;
    add_column_if_missing(
        connection,
        "focus_sessions",
        "today_plan_item_id",
        "INTEGER",
    )?;
    add_column_if_missing(connection, "study_modes", "schedule_block_id", "INTEGER")?;
    add_column_if_missing(connection, "study_modes", "today_plan_item_id", "INTEGER")?;
    add_column_if_missing(connection, "whitelist_apps", "subject_id", "INTEGER")?;
    add_column_if_missing(
        connection,
        "feishu_sync_runs",
        "task_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "feishu_sync_runs",
        "calendar_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    backfill_feishu_task_count(connection)?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sync_runs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              sync_id TEXT NOT NULL,
              backend TEXT NOT NULL,
              trigger TEXT NOT NULL,
              direction TEXT,
              status TEXT NOT NULL,
              started_at TEXT NOT NULL,
              finished_at TEXT NOT NULL,
              duration_ms INTEGER NOT NULL DEFAULT 0,
              device_id TEXT,
              remote_device_id TEXT,
              remote_exported_at INTEGER,
              local_exported_at INTEGER,
              bytes INTEGER NOT NULL DEFAULT 0,
              imported_count INTEGER NOT NULL DEFAULT 0,
              exported_count INTEGER NOT NULL DEFAULT 0,
              deleted_count INTEGER NOT NULL DEFAULT 0,
              conflict_count INTEGER NOT NULL DEFAULT 0,
              active_state_changed INTEGER NOT NULL DEFAULT 0,
              took_over_active_mode INTEGER NOT NULL DEFAULT 0,
              validation_report TEXT,
              backup_path TEXT,
              remote_backup_key TEXT,
              active_snapshot_sync_id TEXT,
              remote_active_snapshot_sync_id TEXT,
              active_snapshot_phase TEXT,
              remote_active_snapshot_phase TEXT,
              active_snapshot_updated_at INTEGER,
              remote_snapshot_updated_at INTEGER,
              remote_exported_drift_seconds INTEGER,
              detail TEXT,
              error_message TEXT
            );

            CREATE TABLE IF NOT EXISTS sync_conflicts (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              run_id INTEGER NOT NULL,
              entity_type TEXT NOT NULL,
              sync_id TEXT NOT NULL,
              resolution TEXT NOT NULL,
              local_updated_at INTEGER,
              remote_updated_at INTEGER,
              message TEXT,
              created_at TEXT NOT NULL,
              FOREIGN KEY (run_id) REFERENCES sync_runs(id)
            );

            CREATE TABLE IF NOT EXISTS sync_outbox (
              op_id TEXT PRIMARY KEY,
              device_id TEXT NOT NULL,
              seq INTEGER NOT NULL,
              hlc TEXT NOT NULL,
              base_hlc TEXT,
              entity_type TEXT NOT NULL,
              sync_id TEXT NOT NULL,
              action TEXT NOT NULL,
              payload_json TEXT,
              deleted_at INTEGER,
              created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
              uploaded_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS sync_applied_ops (
              op_id TEXT PRIMARY KEY,
              device_id TEXT NOT NULL,
              seq INTEGER NOT NULL,
              hlc TEXT NOT NULL,
              applied_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
            );

            CREATE TABLE IF NOT EXISTS sync_device_state (
              device_id TEXT PRIMARY KEY,
              next_seq INTEGER NOT NULL DEFAULT 1,
              last_hlc TEXT,
              updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
            );

            CREATE TABLE IF NOT EXISTS sync_entity_versions (
              entity_type TEXT NOT NULL,
              sync_id TEXT NOT NULL,
              hlc TEXT NOT NULL,
              deleted_at INTEGER,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (entity_type, sync_id)
            );

            CREATE INDEX IF NOT EXISTS idx_sync_meta_sync_id
              ON sync_meta (sync_id);
            CREATE INDEX IF NOT EXISTS idx_sync_meta_entity_deleted
              ON sync_meta (entity_type, deleted_at);
            CREATE INDEX IF NOT EXISTS idx_sync_runs_finished_at
              ON sync_runs (finished_at DESC);
            CREATE INDEX IF NOT EXISTS idx_sync_conflicts_run
              ON sync_conflicts (run_id);
            CREATE INDEX IF NOT EXISTS idx_sync_outbox_device_seq
              ON sync_outbox (device_id, seq);
            CREATE INDEX IF NOT EXISTS idx_sync_outbox_entity
              ON sync_outbox (entity_type, sync_id);
            ",
        )
        .map_err(|error| error.to_string())?;
    add_column_if_missing(connection, "sync_runs", "active_snapshot_sync_id", "TEXT")?;
    add_column_if_missing(
        connection,
        "sync_runs",
        "remote_active_snapshot_sync_id",
        "TEXT",
    )?;
    add_column_if_missing(connection, "sync_runs", "active_snapshot_phase", "TEXT")?;
    add_column_if_missing(
        connection,
        "sync_runs",
        "remote_active_snapshot_phase",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "sync_runs",
        "active_snapshot_updated_at",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "sync_runs",
        "remote_snapshot_updated_at",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "sync_runs",
        "remote_exported_drift_seconds",
        "INTEGER",
    )?;
    add_column_if_missing(connection, "sync_runs", "detail", "TEXT")?;
    migrate_sync_meta_non_unique_sync_id(connection)?;
    connection
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_whitelist_apps_subject ON whitelist_apps (subject_id, enabled)",
            [],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn migrate_sync_meta_non_unique_sync_id(connection: &Connection) -> Result<(), String> {
    if !sync_meta_has_unique_sync_id_index(connection)? {
        return Ok(());
    }

    let has_deleted_at = table_has_column(connection, "sync_meta", "deleted_at")?;
    let has_created_at = table_has_column(connection, "sync_meta", "created_at")?;
    let deleted_expr = if has_deleted_at { "deleted_at" } else { "NULL" };
    let created_expr = if has_created_at {
        "COALESCE(created_at, strftime('%s','now') * 1000)"
    } else {
        "strftime('%s','now') * 1000"
    };
    let created_order_expr = if has_created_at {
        "COALESCE(created_at, 0)"
    } else {
        "0"
    };

    connection
        .execute_batch(
            "
            DROP TABLE IF EXISTS sync_meta_rebuild;
            CREATE TABLE sync_meta_rebuild (
              entity_type TEXT NOT NULL,
              local_id INTEGER NOT NULL,
              sync_id TEXT NOT NULL,
              deleted_at INTEGER,
              created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (entity_type, local_id)
            );
            ",
        )
        .map_err(|error| error.to_string())?;

    connection
        .execute(
            &format!(
                "
                INSERT OR REPLACE INTO sync_meta_rebuild (
                  entity_type, local_id, sync_id, deleted_at, created_at, updated_at
                )
                SELECT entity_type,
                       local_id,
                       sync_id,
                       {deleted_expr},
                       {created_expr},
                       COALESCE(updated_at, strftime('%s','now') * 1000)
                FROM sync_meta
                WHERE entity_type IS NOT NULL
                  AND local_id IS NOT NULL
                  AND sync_id IS NOT NULL
                ORDER BY COALESCE(updated_at, 0) ASC,
                         {created_order_expr} ASC,
                         rowid ASC
                "
            ),
            [],
        )
        .map_err(|error| error.to_string())?;

    connection
        .execute_batch(
            "
            DROP TABLE sync_meta;
            ALTER TABLE sync_meta_rebuild RENAME TO sync_meta;
            CREATE INDEX IF NOT EXISTS idx_sync_meta_sync_id
              ON sync_meta (sync_id);
            CREATE INDEX IF NOT EXISTS idx_sync_meta_entity_deleted
              ON sync_meta (entity_type, deleted_at);
            ",
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn sync_meta_has_unique_sync_id_index(connection: &Connection) -> Result<bool, String> {
    let mut statement = connection
        .prepare("PRAGMA index_list(sync_meta)")
        .map_err(|error| error.to_string())?;
    let indexes = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    for (index_name, unique) in indexes {
        if unique == 0 {
            continue;
        }
        let mut index_statement = connection
            .prepare(&format!(
                "PRAGMA index_info({})",
                sqlite_identifier(&index_name)
            ))
            .map_err(|error| error.to_string())?;
        let columns = index_statement
            .query_map([], |row| row.get::<_, String>(2))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        if columns.len() == 1 && columns[0] == "sync_id" {
            return Ok(true);
        }
    }

    Ok(false)
}

fn sqlite_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    connection
        .execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(columns.iter().any(|existing| existing == column))
}

fn backfill_feishu_task_count(connection: &Connection) -> Result<(), String> {
    if !table_has_column(connection, "feishu_sync_runs", "todo_count")? {
        return Ok(());
    }

    connection
        .execute(
            "
            UPDATE feishu_sync_runs
            SET task_count = todo_count
            WHERE task_count = 0 AND todo_count > 0
            ",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn backfill_study_mode_accumulated_seconds(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE study_modes
            SET accumulated_study_seconds = CASE
                WHEN status != 'active' THEN COALESCE(accumulated_study_seconds, planned_seconds)
                WHEN phase = 'focus' THEN MAX(0, focus_seconds * MAX(cycle_index - 1, 0))
                WHEN phase IN ('awaiting_break', 'break') THEN MIN(planned_seconds, focus_seconds * cycle_index)
                ELSE COALESCE(accumulated_study_seconds, 0)
            END
            WHERE accumulated_study_seconds = 0
              AND (cycle_index > 1 OR phase IN ('awaiting_break', 'break') OR status != 'active')
            ",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn seed_default_subjects(connection: &Connection) -> Result<(), String> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM subjects", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    if count > 0 {
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let defaults = [
        ("政治", "#ef4444"),
        ("英语", "#3b82f6"),
        ("数学", "#16a34a"),
        ("专业课", "#a855f7"),
    ];

    for (name, color) in defaults {
        connection
            .execute(
                "
                INSERT INTO subjects (name, color, enabled, created_at, updated_at)
                VALUES (?1, ?2, 1, ?3, ?3)
                ",
                rusqlite::params![name, color, now],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn normalize_default_subjects(connection: &Connection) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_millis = chrono::Utc::now().timestamp_millis();

    for (sync_id, legacy_sync_id, name, color) in DEFAULT_SUBJECTS {
        let mut statement = connection
            .prepare(
                "
                SELECT DISTINCT s.id
                FROM subjects s
                LEFT JOIN sync_meta m
                  ON m.entity_type = 'subject' AND m.local_id = s.id
                WHERE TRIM(s.name) = ?1
                   OR m.sync_id IN (?2, ?3)
                ORDER BY
                  CASE
                    WHEN m.sync_id = ?2 THEN 0
                    WHEN s.enabled = 1 AND TRIM(s.name) = ?1 THEN 1
                    ELSE 2
                  END,
                  s.id ASC
                ",
            )
            .map_err(|error| error.to_string())?;
        let candidate_ids = statement
            .query_map(params![name, sync_id, legacy_sync_id], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;

        let canonical_id = if let Some(id) = candidate_ids.first().copied() {
            id
        } else {
            connection
                .execute(
                    "
                    INSERT INTO subjects (name, color, enabled, created_at, updated_at)
                    VALUES (?1, ?2, 1, ?3, ?3)
                    ",
                    params![name, color, now],
                )
                .map_err(|error| error.to_string())?;
            connection.last_insert_rowid()
        };

        connection
            .execute(
                "
                UPDATE subjects
                SET name = ?1,
                    color = ?2,
                    enabled = 1,
                    updated_at = CASE
                      WHEN name <> ?1 OR COALESCE(color, '') <> ?2 OR enabled <> 1 THEN ?3
                      ELSE updated_at
                    END
                WHERE id = ?4
                ",
                params![name, color, now, canonical_id],
            )
            .map_err(|error| error.to_string())?;

        let duplicate_ids = candidate_ids
            .into_iter()
            .filter(|id| *id != canonical_id)
            .collect::<Vec<_>>();
        for duplicate_id in duplicate_ids {
            rewrite_subject_references(connection, duplicate_id, canonical_id)?;
            connection
                .execute(
                    "
                    UPDATE subjects
                    SET enabled = 0,
                        updated_at = ?1
                    WHERE id = ?2
                    ",
                    params![now, duplicate_id],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "
                    DELETE FROM sync_meta
                    WHERE entity_type = 'subject'
                      AND local_id = ?1
                    ",
                    params![duplicate_id],
                )
                .map_err(|error| error.to_string())?;
        }

        connection
            .execute(
                "
                DELETE FROM sync_meta
                WHERE entity_type = 'subject'
                  AND (sync_id IN (?1, ?2) OR local_id = ?3)
                ",
                params![sync_id, legacy_sync_id, canonical_id],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "
                INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
                VALUES ('subject', ?1, ?2, NULL, ?3)
                ON CONFLICT(entity_type, local_id) DO UPDATE SET
                  sync_id = excluded.sync_id,
                  deleted_at = excluded.deleted_at,
                  updated_at = excluded.updated_at
                ",
                params![canonical_id, sync_id, now_millis],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn rewrite_subject_references(
    connection: &Connection,
    old_subject_id: i64,
    canonical_subject_id: i64,
) -> Result<(), String> {
    for table in [
        "focus_sessions",
        "study_modes",
        "whitelist_apps",
        "checklist_tasks",
        "today_plan_items",
        "schedule_templates",
        "schedule_blocks",
    ] {
        connection
            .execute(
                &format!("UPDATE {table} SET subject_id = ?1 WHERE subject_id = ?2"),
                params![canonical_subject_id, old_subject_id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

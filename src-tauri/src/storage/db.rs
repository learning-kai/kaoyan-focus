use rusqlite::Connection;
use std::{fs, path::Path};

pub fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    run_migrations(&connection)?;
    seed_default_subjects(&connection)?;
    Ok(connection)
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
              phase TEXT NOT NULL,
              cycle_index INTEGER NOT NULL DEFAULT 1,
              started_at TEXT NOT NULL,
              phase_started_at TEXT NOT NULL,
              paused_at TEXT,
              total_paused_seconds INTEGER NOT NULL DEFAULT 0,
              phase_paused_seconds INTEGER NOT NULL DEFAULT 0,
              ended_at TEXT,
              current_session_id INTEGER,
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
              PRIMARY KEY (entity_type, local_id),
              UNIQUE(sync_id)
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

            CREATE INDEX IF NOT EXISTS idx_checklist_columns_scope_sort
              ON checklist_columns (board_scope, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_checklist_tasks_column_sort
              ON checklist_tasks (column_id, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_checklist_tasks_scope_sort
              ON checklist_tasks (board_scope, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_today_plan_items_date_sort
              ON today_plan_items (today_date, sort_order, id);
            CREATE INDEX IF NOT EXISTS idx_sync_meta_sync_id
              ON sync_meta (sync_id);
            CREATE INDEX IF NOT EXISTS idx_sync_meta_entity_deleted
              ON sync_meta (entity_type, deleted_at);
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
    add_column_if_missing(connection, "study_modes", "paused_at", "TEXT")?;
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

    Ok(())
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

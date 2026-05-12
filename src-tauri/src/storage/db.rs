use rusqlite::Connection;
use std::{fs, path::Path};

pub fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    run_migrations(&connection)?;
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
              started_at TEXT NOT NULL,
              ended_at TEXT,
              status TEXT NOT NULL,
              end_reason TEXT,
              interruption_count INTEGER NOT NULL DEFAULT 0,
              emergency_exit_count INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|error| error.to_string())
}

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeHealth {
    pub status: String,
    pub summary: String,
    pub checked_at: String,
    pub protected_storage: RuntimeHealthCheck,
    pub checks: Vec<RuntimeHealthCheck>,
    pub generated_at: String,
    pub tasks: Vec<RuntimeTaskHealth>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeTaskHealth {
    pub task: String,
    pub status: String,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub next_retry_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeHealthCheck {
    pub key: String,
    pub label: String,
    pub status: String,
    pub message: Option<String>,
    pub detail: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone)]
struct RuntimeTaskSnapshot {
    status: String,
    last_success_at: Option<String>,
    last_error: Option<String>,
    next_retry_at: Option<String>,
}

static RUNTIME_TASKS: OnceLock<Mutex<HashMap<&'static str, RuntimeTaskSnapshot>>> = OnceLock::new();

pub fn mark_task_success(task: &'static str, next_retry_seconds: Option<i64>) {
    let now = Utc::now();
    set_task_snapshot(
        task,
        RuntimeTaskSnapshot {
            status: "ok".to_string(),
            last_success_at: Some(now.to_rfc3339()),
            last_error: None,
            next_retry_at: next_retry_seconds
                .map(|seconds| (now + Duration::seconds(seconds)).to_rfc3339()),
        },
    );
}

pub fn mark_task_error(task: &'static str, error: &str, next_retry_seconds: Option<i64>) {
    let now = Utc::now();
    set_task_snapshot(
        task,
        RuntimeTaskSnapshot {
            status: "error".to_string(),
            last_success_at: None,
            last_error: Some(error.to_string()),
            next_retry_at: next_retry_seconds
                .map(|seconds| (now + Duration::seconds(seconds)).to_rfc3339()),
        },
    );
}

pub fn runtime_health_from_database(connection: &Connection) -> Result<RuntimeHealth, String> {
    let mut tasks = vec![
        sync_task_health(connection, "webdav", "webdav_sync")?,
        sync_task_health(connection, "object_storage", "object_storage_sync")?,
        feishu_task_health(connection)?,
        email_task_health(connection)?,
        whitelist_task_health(connection)?,
    ];

    let snapshots = RUNTIME_TASKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    for (task, snapshot) in snapshots {
        upsert_task(
            &mut tasks,
            RuntimeTaskHealth {
                task: task.to_string(),
                status: snapshot.status,
                last_success_at: snapshot.last_success_at,
                last_error: snapshot.last_error,
                next_retry_at: snapshot.next_retry_at,
            },
        );
    }

    let checked_at = Utc::now().to_rfc3339();
    let protected_storage = protected_storage_check(&checked_at);
    let mut checks = Vec::new();
    checks.push(protected_storage.clone());
    checks.extend(tasks.iter().map(|task| task_check(task, &checked_at)));
    let status = overall_status(&checks);
    let summary = match status.as_str() {
        "ok" => "Runtime background tasks are reporting normally.",
        "warning" => "Some runtime tasks have not reported yet.",
        "error" => "One or more runtime tasks reported an error.",
        _ => "Runtime health is unavailable.",
    }
    .to_string();

    Ok(RuntimeHealth {
        status,
        summary,
        checked_at: checked_at.clone(),
        protected_storage,
        checks,
        generated_at: checked_at,
        tasks,
    })
}

fn set_task_snapshot(task: &'static str, snapshot: RuntimeTaskSnapshot) {
    if let Ok(mut tasks) = RUNTIME_TASKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        tasks.insert(task, snapshot);
    }
}

fn sync_task_health(
    connection: &Connection,
    backend: &str,
    task: &str,
) -> Result<RuntimeTaskHealth, String> {
    let row: Option<(String, String, Option<String>)> = connection
        .query_row(
            "
            SELECT status, finished_at, error_message
            FROM sync_runs
            WHERE backend = ?1
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            ",
            [backend],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(match row {
        Some((status, finished_at, error_message)) => {
            task_from_run(task, status, finished_at, error_message, Some(120))
        }
        None => never_run_task(task),
    })
}

fn feishu_task_health(connection: &Connection) -> Result<RuntimeTaskHealth, String> {
    let row: Option<(String, String, Option<String>)> = connection
        .query_row(
            "
            SELECT status, finished_at, error_message
            FROM feishu_sync_runs
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            ",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(match row {
        Some((status, finished_at, error_message)) => {
            task_from_run("feishu_sync", status, finished_at, error_message, Some(60))
        }
        None => never_run_task("feishu_sync"),
    })
}

fn email_task_health(connection: &Connection) -> Result<RuntimeTaskHealth, String> {
    let sent_at: Option<String> = connection
        .query_row(
            "
            SELECT sent_at
            FROM email_notification_logs
            ORDER BY sent_at DESC, id DESC
            LIMIT 1
            ",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(RuntimeTaskHealth {
        task: "email_reminder".to_string(),
        status: sent_at
            .as_ref()
            .map(|_| "ok")
            .unwrap_or("not_run")
            .to_string(),
        last_success_at: sent_at,
        last_error: None,
        next_retry_at: next_local_21_clock_utc(),
    })
}

fn whitelist_task_health(connection: &Connection) -> Result<RuntimeTaskHealth, String> {
    let row: Option<String> = connection
        .query_row(
            "
            SELECT created_at
            FROM app_events
            WHERE event_type = 'blocked_foreground_detected'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(RuntimeTaskHealth {
        task: "whitelist_guard".to_string(),
        status: "ok".to_string(),
        last_success_at: row,
        last_error: None,
        next_retry_at: Some((Utc::now() + Duration::seconds(3)).to_rfc3339()),
    })
}

fn task_from_run(
    task: &str,
    run_status: String,
    finished_at: String,
    error_message: Option<String>,
    next_retry_seconds: Option<i64>,
) -> RuntimeTaskHealth {
    let is_error = run_status == "failed"
        || error_message
            .as_ref()
            .is_some_and(|value| !value.is_empty());
    RuntimeTaskHealth {
        task: task.to_string(),
        status: if is_error { "error" } else { "ok" }.to_string(),
        last_success_at: (!is_error).then_some(finished_at),
        last_error: is_error.then(|| error_message.unwrap_or(run_status)),
        next_retry_at: next_retry_seconds
            .map(|seconds| (Utc::now() + Duration::seconds(seconds)).to_rfc3339()),
    }
}

fn never_run_task(task: &str) -> RuntimeTaskHealth {
    RuntimeTaskHealth {
        task: task.to_string(),
        status: "not_run".to_string(),
        last_success_at: None,
        last_error: None,
        next_retry_at: None,
    }
}

fn upsert_task(tasks: &mut Vec<RuntimeTaskHealth>, next: RuntimeTaskHealth) {
    if let Some(existing) = tasks.iter_mut().find(|item| item.task == next.task) {
        if existing.last_success_at.is_none() {
            existing.last_success_at = next.last_success_at.clone();
        }
        *existing = RuntimeTaskHealth {
            last_success_at: existing.last_success_at.clone().or(next.last_success_at),
            ..next
        };
    } else {
        tasks.push(next);
    }
}

fn protected_storage_check(checked_at: &str) -> RuntimeHealthCheck {
    RuntimeHealthCheck {
        key: "protected_storage".to_string(),
        label: "Protected credential storage".to_string(),
        status: if cfg!(windows) { "ok" } else { "unavailable" }.to_string(),
        message: Some(if cfg!(windows) {
            "Windows DPAPI credential protection is available.".to_string()
        } else {
            "Protected credential storage is only implemented for Windows.".to_string()
        }),
        detail: Some("Secrets are stored in SQLite only as dpapi:v1 protected payloads after save or migration-on-read.".to_string()),
        checked_at: checked_at.to_string(),
    }
}

fn task_check(task: &RuntimeTaskHealth, checked_at: &str) -> RuntimeHealthCheck {
    RuntimeHealthCheck {
        key: task.task.clone(),
        label: task.task.replace('_', " "),
        status: match task.status.as_str() {
            "ok" => "ok",
            "error" => "error",
            "not_run" => "warning",
            _ => "unknown",
        }
        .to_string(),
        message: task
            .last_error
            .clone()
            .or_else(|| Some(format!("status={}", task.status))),
        detail: task
            .next_retry_at
            .as_ref()
            .map(|value| format!("next_retry_at={value}")),
        checked_at: task
            .last_success_at
            .clone()
            .unwrap_or_else(|| checked_at.to_string()),
    }
}

fn overall_status(checks: &[RuntimeHealthCheck]) -> String {
    if checks
        .iter()
        .any(|check| check.status == "error" || check.status == "failed")
    {
        "error".to_string()
    } else if checks
        .iter()
        .any(|check| check.status == "warning" || check.status == "unavailable")
    {
        "warning".to_string()
    } else {
        "ok".to_string()
    }
}

fn next_local_21_clock_utc() -> Option<String> {
    let now = chrono::Local::now();
    let today_21 = now.date_naive().and_hms_opt(21, 0, 0)?;
    let next = if now.naive_local() < today_21 {
        today_21
    } else {
        (now.date_naive() + Duration::days(1)).and_hms_opt(21, 0, 0)?
    };
    next.and_local_timezone(*now.offset())
        .single()
        .map(|value| value.with_timezone(&Utc).to_rfc3339())
}

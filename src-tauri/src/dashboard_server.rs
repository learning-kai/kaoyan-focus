use chrono::{DateTime, FixedOffset, Timelike, Utc};
use rusqlite::{Connection, OpenFlags, Row};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const INDEX_HTML: &str = include_str!("../dashboard/index.html");
const APP_JS: &str = include_str!("../dashboard/app.js");
const STYLES_CSS: &str = include_str!("../dashboard/styles.css");
const DATABASE_NAME: &str = "kaoyan-focus.sqlite3";

static DASHBOARD_SERVER: OnceLock<Mutex<Option<DashboardServerState>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct DashboardServerState {
    port: u16,
    token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLaunch {
    pub url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyDataPayload {
    read_only: bool,
    source: Option<StudyDataSource>,
    records: Vec<StudyDataRecord>,
    generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyDataSource {
    #[serde(rename = "type")]
    source_type: String,
    path: String,
    bytes: u64,
    last_modified: String,
    subject_count: i64,
    task_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyDataRecord {
    id: String,
    date: String,
    subject: String,
    minutes: i64,
    focus_score: i64,
    tasks_done: i64,
    tasks_total: i64,
    start_hour: u32,
}

#[derive(Debug, Clone)]
struct TaskBucket {
    date: String,
    subject_id: Option<i64>,
    done: i64,
    total: i64,
}

pub fn ensure_running(app: AppHandle) -> Result<DashboardLaunch, String> {
    let store = DASHBOARD_SERVER.get_or_init(|| Mutex::new(None));
    let mut server_guard = store.lock().map_err(|error| error.to_string())?;

    if let Some(server) = server_guard.as_ref() {
        return Ok(DashboardLaunch {
            url: dashboard_url(server.port, &server.token),
        });
    }

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let token = Uuid::new_v4().simple().to_string();
    let server_app = app.clone();
    let server_token = token.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let request_app = server_app.clone();
                    let request_token = server_token.clone();
                    thread::spawn(move || handle_connection(stream, request_app, request_token));
                }
                Err(error) => eprintln!("Study dashboard local server error: {error}"),
            }
        }
    });

    *server_guard = Some(DashboardServerState {
        port,
        token: token.clone(),
    });
    Ok(DashboardLaunch {
        url: dashboard_url(port, &token),
    })
}

fn dashboard_url(port: u16, token: &str) -> String {
    format!("http://127.0.0.1:{port}/?token={token}")
}

fn handle_connection(mut stream: TcpStream, app: AppHandle, token: String) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let mut buffer = [0; 8192];
    let read = match stream.read(&mut buffer) {
        Ok(read) if read > 0 => read,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(request_line) = request.lines().next() else {
        let _ = send_plain_response(
            &mut stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            "Bad Request",
            false,
        );
        return;
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");

    match method {
        "GET" | "HEAD" => handle_get(&mut stream, &app, method == "HEAD", path, target, &token),
        "POST" | "PUT" | "PATCH" | "DELETE" => {
            let body = serde_json::json!({
                "ok": false,
                "readOnly": true,
                "error": "This dashboard server is read-only."
            })
            .to_string();
            let _ = send_plain_response(
                &mut stream,
                405,
                "Method Not Allowed",
                "application/json; charset=utf-8",
                &body,
                method == "HEAD",
            );
        }
        _ => {
            let _ = send_plain_response(
                &mut stream,
                405,
                "Method Not Allowed",
                "application/json; charset=utf-8",
                r#"{"ok":false,"readOnly":true,"error":"Method not allowed."}"#,
                method == "HEAD",
            );
        }
    }
}

fn handle_get(
    stream: &mut TcpStream,
    app: &AppHandle,
    head_only: bool,
    path: &str,
    target: &str,
    token: &str,
) {
    let response = match path {
        "/" | "/index.html" => send_plain_response(
            stream,
            200,
            "OK",
            "text/html; charset=utf-8",
            INDEX_HTML,
            head_only,
        ),
        "/app.js" => send_plain_response(
            stream,
            200,
            "OK",
            "application/javascript; charset=utf-8",
            APP_JS,
            head_only,
        ),
        "/styles.css" => send_plain_response(
            stream,
            200,
            "OK",
            "text/css; charset=utf-8",
            STYLES_CSS,
            head_only,
        ),
        "/api/health" => send_json_response(
            stream,
            200,
            "OK",
            &serde_json::json!({ "ok": true, "readOnly": true }),
            head_only,
        ),
        "/api/study-data" => {
            if !has_valid_dashboard_token(target, token) {
                send_json_response(
                    stream,
                    401,
                    "Unauthorized",
                    &serde_json::json!({
                        "readOnly": true,
                        "error": "Dashboard token is missing or invalid."
                    }),
                    head_only,
                )
            } else {
                match build_payload(app) {
                    Ok(payload) => send_json_response(stream, 200, "OK", &payload, head_only),
                    Err(error) => send_json_response(
                        stream,
                        200,
                        "OK",
                        &StudyDataPayload {
                            read_only: true,
                            source: None,
                            records: Vec::new(),
                            generated_at: now_local_rfc3339(),
                            error: Some(error),
                        },
                        head_only,
                    ),
                }
            }
        }
        _ => send_plain_response(
            stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            "Not Found",
            head_only,
        ),
    };

    if let Err(error) = response {
        eprintln!("Study dashboard response error: {error}");
    }
}

fn has_valid_dashboard_token(target: &str, expected_token: &str) -> bool {
    target
        .split_once('?')
        .map(|(_, query)| {
            query.split('&').any(|part| {
                let (key, value) = part.split_once('=').unwrap_or((part, ""));
                key == "token" && value == expected_token
            })
        })
        .unwrap_or(false)
}

fn send_json_response<T: Serialize>(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    payload: &T,
    head_only: bool,
) -> std::io::Result<()> {
    let body = serde_json::to_string(payload).unwrap_or_else(|_| {
        r#"{"ok":false,"readOnly":true,"error":"Failed to serialize response."}"#.to_string()
    });
    send_plain_response(
        stream,
        status,
        reason,
        "application/json; charset=utf-8",
        &body,
        head_only,
    )
}

fn send_plain_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
    head_only: bool,
) -> std::io::Result<()> {
    let bytes = body.as_bytes();
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream.write_all(headers.as_bytes())?;
    if !head_only {
        stream.write_all(bytes)?;
    }
    stream.flush()
}

fn build_payload(app: &AppHandle) -> Result<StudyDataPayload, String> {
    let database_path = database_path(app)?;
    if !database_path.exists() {
        return Err(format!("未找到 {}", DATABASE_NAME));
    }

    let metadata = fs::metadata(&database_path).map_err(|error| error.to_string())?;
    let connection = open_readonly_database(&database_path)?;
    let records = load_records(&connection)?;
    let subject_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM subjects", [], |row| row.get(0))
        .unwrap_or(0);
    let task_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM today_plan_items", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(StudyDataPayload {
        read_only: true,
        source: Some(StudyDataSource {
            source_type: "sqlite".to_string(),
            path: database_path.to_string_lossy().to_string(),
            bytes: metadata.len(),
            last_modified: metadata
                .modified()
                .map(format_system_time)
                .unwrap_or_else(|_| now_local_rfc3339()),
            subject_count,
            task_count,
        }),
        records,
        generated_at: now_local_rfc3339(),
        error: None,
    })
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(DATABASE_NAME))
}

fn open_readonly_database(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

fn load_task_buckets(connection: &Connection) -> Result<Vec<TaskBucket>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT today_date,
                   subject_id,
                   COUNT(*) AS total,
                   SUM(CASE WHEN completed = 1 THEN 1 ELSE 0 END) AS done
            FROM today_plan_items
            GROUP BY today_date, subject_id
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(TaskBucket {
                date: row.get(0)?,
                subject_id: row.get(1)?,
                total: row.get(2)?,
                done: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut buckets = Vec::new();
    for row in rows {
        buckets.push(row.map_err(|error| error.to_string())?);
    }
    Ok(buckets)
}

fn load_records(connection: &Connection) -> Result<Vec<StudyDataRecord>, String> {
    let task_buckets = load_task_buckets(connection)?;
    let mut statement = connection
        .prepare(
            "
            SELECT fs.id,
                   fs.subject_id,
                   COALESCE(NULLIF(TRIM(s.name), ''), '未命名科目') AS subject,
                   fs.planned_seconds,
                   fs.actual_seconds,
                   fs.started_at,
                   fs.status,
                   fs.end_reason,
                   fs.interruption_count,
                   fs.emergency_exit_count,
                   fs.paused_seconds
            FROM focus_sessions fs
            LEFT JOIN subjects s ON s.id = fs.subject_id
            WHERE COALESCE(fs.actual_seconds, 0) > 0
              AND fs.started_at IS NOT NULL
            ORDER BY fs.started_at ASC, fs.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| map_focus_session(row, &task_buckets))
        .map_err(|error| error.to_string())?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| error.to_string())?);
    }
    Ok(records)
}

fn map_focus_session(row: &Row<'_>, task_buckets: &[TaskBucket]) -> rusqlite::Result<StudyDataRecord> {
    let id: i64 = row.get(0)?;
    let subject_id: Option<i64> = row.get(1)?;
    let subject: String = row.get(2)?;
    let planned_seconds: i64 = row.get(3)?;
    let actual_seconds: i64 = row.get(4)?;
    let started_at: String = row.get(5)?;
    let status: String = row.get(6)?;
    let end_reason: Option<String> = row.get(7)?;
    let interruption_count: i64 = row.get(8)?;
    let emergency_exit_count: i64 = row.get(9)?;
    let paused_seconds: i64 = row.get(10)?;

    let started = parse_local_datetime(&started_at)
        .unwrap_or_else(|| Utc::now().with_timezone(&local_offset()));
    let minutes = (actual_seconds.max(0) + 30) / 60;
    let minutes = if actual_seconds > 0 && minutes == 0 { 1 } else { minutes };
    let date = started.date_naive().to_string();
    let session_finished = status == "finished" || end_reason.as_deref() == Some("completed");
    let (tasks_done, tasks_total) =
        resolve_task_bucket(task_buckets, &date, subject_id, session_finished);

    Ok(StudyDataRecord {
        id: format!("focus-session:{id}"),
        date,
        subject,
        minutes,
        focus_score: focus_score(
            planned_seconds,
            actual_seconds,
            &status,
            end_reason.as_deref(),
            interruption_count,
            emergency_exit_count,
            paused_seconds,
        ),
        tasks_done,
        tasks_total,
        start_hour: started.time().hour(),
    })
}

fn resolve_task_bucket(
    buckets: &[TaskBucket],
    date: &str,
    subject_id: Option<i64>,
    session_finished: bool,
) -> (i64, i64) {
    if let Some(bucket) = buckets
        .iter()
        .find(|bucket| bucket.date == date && bucket.subject_id == subject_id && bucket.total > 0)
        .or_else(|| {
            buckets
                .iter()
                .find(|bucket| bucket.date == date && bucket.subject_id.is_none() && bucket.total > 0)
        })
    {
        return (bucket.done, bucket.total);
    }

    (if session_finished { 1 } else { 0 }, 1)
}

fn focus_score(
    planned_seconds: i64,
    actual_seconds: i64,
    status: &str,
    end_reason: Option<&str>,
    interruptions: i64,
    emergencies: i64,
    paused_seconds: i64,
) -> i64 {
    let actual = actual_seconds.max(0) as f64;
    let planned = planned_seconds.max(60) as f64;
    let meaningful_planned = clamp(planned, 20.0 * 60.0, 120.0 * 60.0);
    let fit_ratio = if meaningful_planned > 0.0 {
        actual / meaningful_planned
    } else {
        0.0
    };
    let log_fit = if fit_ratio > 0.0 { fit_ratio.ln().abs() } else { 4.0 };

    let duration_quality = 1.0 - (-actual / 3000.0).exp();
    let fit_quality = (-log_fit.powi(2) / (2.0 * 0.42f64.powi(2))).exp();
    let engagement_quality = 0.58 * duration_quality + 0.42 * fit_quality;

    let actual_hours = (actual / 3600.0).max(0.25);
    let interruption_load = interruptions.max(0) as f64 / actual_hours;
    let stability_quality = 1.0
        / (1.0 + interruption_load * 0.72 + emergencies.max(0) as f64 * 1.9);
    let pause_ratio = clamp(paused_seconds.max(0) as f64 / meaningful_planned, 0.0, 1.5);
    let pause_quality = 1.0 / (1.0 + pause_ratio * 1.35);
    let termination_quality = match (status, end_reason) {
        (_, Some("emergency_exit")) | ("emergency_exited", _) => 0.08,
        ("interrupted", _) | (_, Some("user_marked_interrupted")) => 0.62,
        ("finished", _) | (_, Some("completed")) => 0.96,
        ("running", _) => 0.54,
        _ => 0.78,
    };

    let mut score = 100.0
        * (0.58 * engagement_quality
            + 0.20 * stability_quality
            + 0.08 * pause_quality
            + 0.14 * termination_quality);

    let evidence_quality = 0.75 + 0.25 * clamp(actual / 1800.0, 0.0, 1.0);
    score *= evidence_quality;

    if matches!((status, end_reason), (_, Some("emergency_exit")) | ("emergency_exited", _)) {
        score *= 0.85;
    }

    if fit_ratio > 1.3 {
        score -= clamp((fit_ratio - 1.3) / 0.7, 0.0, 1.0) * 6.0;
    }

    clamp(score, 0.0, 100.0).round() as i64
}

fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.max(low).min(high)
}

fn parse_local_datetime(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&local_offset()))
}

fn format_system_time(value: std::time::SystemTime) -> String {
    let utc: DateTime<Utc> = value.into();
    utc.with_timezone(&local_offset()).to_rfc3339()
}

fn now_local_rfc3339() -> String {
    Utc::now().with_timezone(&local_offset()).to_rfc3339()
}

fn local_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 60 * 60).expect("Asia/Shanghai offset should be valid")
}

#[cfg(test)]
mod tests {
    use super::focus_score;

    #[test]
    fn completed_planned_session_scores_high_but_not_magic() {
        let score = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 0);

        assert!((80..=90).contains(&score), "score={score}");
    }

    #[test]
    fn tiny_completed_session_does_not_look_like_deep_focus() {
        let tiny = focus_score(5 * 60, 5 * 60, "finished", Some("completed"), 0, 0, 0);
        let full = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 0);

        assert!(tiny < full, "tiny={tiny}, full={full}");
        assert!(tiny <= 38, "tiny={tiny}");
    }

    #[test]
    fn interruptions_are_weighted_by_focus_density() {
        let clean = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 0);
        let noisy = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 4, 0, 0);

        assert!(clean - noisy >= 12, "clean={clean}, noisy={noisy}");
    }

    #[test]
    fn early_stop_is_substantially_lower_than_completion() {
        let stopped = focus_score(45 * 60, 15 * 60, "interrupted", Some("user_marked_interrupted"), 1, 0, 0);
        let complete = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 0);

        assert!(stopped < complete - 30, "stopped={stopped}, complete={complete}");
    }

    #[test]
    fn emergency_exit_has_strong_penalty() {
        let score = focus_score(45 * 60, 30 * 60, "emergency_exited", Some("emergency_exit"), 1, 1, 0);

        assert!(score <= 40, "score={score}");
    }

    #[test]
    fn pause_time_reduces_quality_without_erasing_the_session() {
        let clean = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 0);
        let paused = focus_score(45 * 60, 45 * 60, "finished", Some("completed"), 0, 0, 20 * 60);

        assert!(paused < clean, "clean={clean}, paused={paused}");
        assert!(paused > 50, "paused={paused}");
    }
}

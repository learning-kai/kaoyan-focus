use crate::storage::db::open_database;
use chrono::{Duration, Local, Timelike, Utc};
use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, Message, SmtpTransport,
    Transport,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const EMAIL_REMINDER_ENABLED_KEY: &str = "email_reminder_enabled";
const SMTP_HOST_KEY: &str = "smtp_host";
const SMTP_PORT_KEY: &str = "smtp_port";
const SMTP_SECURITY_KEY: &str = "smtp_security";
const SMTP_USERNAME_KEY: &str = "smtp_username";
const SMTP_PASSWORD_KEY: &str = "smtp_password";
const SMTP_FROM_KEY: &str = "smtp_from";
const SMTP_TO_KEY: &str = "smtp_to";
const REMINDER_TYPE_DUE_TOMORROW_21: &str = "due_tomorrow_21";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailReminderSettings {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: i64,
    pub smtp_security: String,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmailReminderResult {
    pub status: String,
    pub message: String,
    pub sent_count: i64,
}

#[derive(Debug, Clone)]
struct DueTask {
    entity_type: &'static str,
    entity_id: i64,
    title: String,
    due_date: String,
}

impl Default for EmailReminderSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_security: "tls".to_string(),
            username: String::new(),
            password: String::new(),
            from: String::new(),
            to: String::new(),
        }
    }
}

#[tauri::command]
pub fn get_email_reminder_settings(app: AppHandle) -> Result<EmailReminderSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    read_email_settings(&connection)
}

#[tauri::command]
pub fn save_email_reminder_settings(
    app: AppHandle,
    settings: EmailReminderSettings,
) -> Result<EmailReminderSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let normalized = normalize_settings(settings)?;
    let now = Utc::now().to_rfc3339();
    set_setting(
        &connection,
        EMAIL_REMINDER_ENABLED_KEY,
        if normalized.enabled { "true" } else { "false" },
        &now,
    )?;
    set_setting(&connection, SMTP_HOST_KEY, &normalized.smtp_host, &now)?;
    set_setting(
        &connection,
        SMTP_PORT_KEY,
        &normalized.smtp_port.to_string(),
        &now,
    )?;
    set_setting(
        &connection,
        SMTP_SECURITY_KEY,
        &normalized.smtp_security,
        &now,
    )?;
    set_setting(&connection, SMTP_USERNAME_KEY, &normalized.username, &now)?;
    set_setting(&connection, SMTP_PASSWORD_KEY, &normalized.password, &now)?;
    set_setting(&connection, SMTP_FROM_KEY, &normalized.from, &now)?;
    set_setting(&connection, SMTP_TO_KEY, &normalized.to, &now)?;
    Ok(normalized)
}

#[tauri::command]
pub fn test_email_reminder(
    _app: AppHandle,
    settings: EmailReminderSettings,
) -> Result<EmailReminderResult, String> {
    let normalized = normalize_settings(settings)?;
    validate_send_settings(&normalized)?;
    let subject = "考研专注邮件提醒测试";
    let body = "这是一封测试邮件。收到它就说明 SMTP 配置可以正常发送提醒。";
    send_email(&normalized, subject, body)?;
    Ok(EmailReminderResult {
        status: "sent".to_string(),
        message: "测试邮件已发送。".to_string(),
        sent_count: 1,
    })
}

#[tauri::command]
pub fn check_due_task_email_reminders(app: AppHandle) -> Result<EmailReminderResult, String> {
    let connection = open_database(&database_path(&app)?)?;
    let settings = read_email_settings(&connection)?;
    if !settings.enabled {
        return Ok(skipped("邮件提醒未开启。"));
    }
    if !is_after_reminder_time() {
        return Ok(skipped("还没到 21:00，暂不发送。"));
    }
    validate_send_settings(&settings)?;

    let tomorrow = (Local::now().date_naive() + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let tasks = load_due_tasks(&connection, &tomorrow)?;
    let mut pending = Vec::new();
    for task in tasks {
        if !has_sent_log(&connection, &task)? {
            pending.push(task);
        }
    }
    if pending.is_empty() {
        return Ok(EmailReminderResult {
            status: "skipped".to_string(),
            message: "没有需要发送的明日到期任务，或已经发送过。".to_string(),
            sent_count: 0,
        });
    }

    let body = build_due_task_email_body(&tomorrow, &pending);
    send_email(
        &settings,
        &format!("明天到期任务提醒（{}）", tomorrow),
        &body,
    )?;

    let sent_at = Utc::now().to_rfc3339();
    for task in &pending {
        insert_sent_log(&connection, task, &sent_at, &settings.to)?;
    }

    Ok(EmailReminderResult {
        status: "sent".to_string(),
        message: format!("已发送 {} 个明日到期任务提醒。", pending.len()),
        sent_count: pending.len() as i64,
    })
}

fn read_email_settings(connection: &Connection) -> Result<EmailReminderSettings, String> {
    let defaults = EmailReminderSettings::default();
    Ok(EmailReminderSettings {
        enabled: get_string_setting(connection, EMAIL_REMINDER_ENABLED_KEY, "false")? == "true",
        smtp_host: get_string_setting(connection, SMTP_HOST_KEY, &defaults.smtp_host)?,
        smtp_port: get_i64_setting(connection, SMTP_PORT_KEY, defaults.smtp_port)?.clamp(1, 65535),
        smtp_security: normalize_security(&get_string_setting(
            connection,
            SMTP_SECURITY_KEY,
            &defaults.smtp_security,
        )?),
        username: get_string_setting(connection, SMTP_USERNAME_KEY, &defaults.username)?,
        password: get_string_setting(connection, SMTP_PASSWORD_KEY, &defaults.password)?,
        from: get_string_setting(connection, SMTP_FROM_KEY, &defaults.from)?,
        to: get_string_setting(connection, SMTP_TO_KEY, &defaults.to)?,
    })
}

fn normalize_settings(settings: EmailReminderSettings) -> Result<EmailReminderSettings, String> {
    Ok(EmailReminderSettings {
        enabled: settings.enabled,
        smtp_host: settings.smtp_host.trim().to_string(),
        smtp_port: settings.smtp_port.clamp(1, 65535),
        smtp_security: normalize_security(&settings.smtp_security),
        username: settings.username.trim().to_string(),
        password: settings.password,
        from: settings.from.trim().to_string(),
        to: settings.to.trim().to_string(),
    })
}

fn normalize_security(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => "none".to_string(),
        "starttls" => "starttls".to_string(),
        _ => "tls".to_string(),
    }
}

fn validate_send_settings(settings: &EmailReminderSettings) -> Result<(), String> {
    if settings.smtp_host.is_empty()
        || settings.username.is_empty()
        || settings.password.is_empty()
        || settings.from.is_empty()
        || settings.to.is_empty()
    {
        return Err("SMTP 配置不完整。".to_string());
    }
    let _: Mailbox = settings
        .from
        .parse()
        .map_err(|_| "发件人邮箱格式不正确。".to_string())?;
    let _: Mailbox = settings
        .to
        .parse()
        .map_err(|_| "收件人邮箱格式不正确。".to_string())?;
    Ok(())
}

fn send_email(settings: &EmailReminderSettings, subject: &str, body: &str) -> Result<(), String> {
    let from: Mailbox = settings
        .from
        .parse()
        .map_err(|_| "发件人邮箱格式不正确。".to_string())?;
    let to: Mailbox = settings
        .to
        .parse()
        .map_err(|_| "收件人邮箱格式不正确。".to_string())?;
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .body(body.to_string())
        .map_err(|error| error.to_string())?;
    let credentials = Credentials::new(settings.username.clone(), settings.password.clone());
    let builder = match settings.smtp_security.as_str() {
        "none" => SmtpTransport::builder_dangerous(&settings.smtp_host),
        "starttls" => {
            SmtpTransport::starttls_relay(&settings.smtp_host).map_err(|error| error.to_string())?
        }
        _ => SmtpTransport::relay(&settings.smtp_host).map_err(|error| error.to_string())?,
    };
    let mailer = builder
        .port(settings.smtp_port as u16)
        .credentials(credentials)
        .build();
    mailer.send(&message).map_err(|error| error.to_string())?;
    Ok(())
}

fn load_due_tasks(connection: &Connection, due_date: &str) -> Result<Vec<DueTask>, String> {
    let mut tasks = Vec::new();
    let mut checklist_statement = connection
        .prepare(
            "
            SELECT id, title, due_date
            FROM checklist_tasks
            WHERE due_date = ?1 AND completed = 0
            ORDER BY sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let checklist_rows = checklist_statement
        .query_map(params![due_date], |row| {
            Ok(DueTask {
                entity_type: "checklist_task",
                entity_id: row.get(0)?,
                title: row.get(1)?,
                due_date: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in checklist_rows {
        tasks.push(row.map_err(|error| error.to_string())?);
    }

    let mut today_statement = connection
        .prepare(
            "
            SELECT id, title, due_date
            FROM today_plan_items
            WHERE due_date = ?1 AND completed = 0
            ORDER BY sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let today_rows = today_statement
        .query_map(params![due_date], |row| {
            Ok(DueTask {
                entity_type: "today_plan_item",
                entity_id: row.get(0)?,
                title: row.get(1)?,
                due_date: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in today_rows {
        tasks.push(row.map_err(|error| error.to_string())?);
    }

    Ok(tasks)
}

fn has_sent_log(connection: &Connection, task: &DueTask) -> Result<bool, String> {
    connection
        .query_row(
            "
            SELECT 1
            FROM email_notification_logs
            WHERE entity_type = ?1
              AND entity_id = ?2
              AND due_date = ?3
              AND reminder_type = ?4
            LIMIT 1
            ",
            params![
                task.entity_type,
                task.entity_id,
                task.due_date,
                REMINDER_TYPE_DUE_TOMORROW_21
            ],
            |_| Ok(()),
        )
        .optional()
        .map(|item| item.is_some())
        .map_err(|error| error.to_string())
}

fn insert_sent_log(
    connection: &Connection,
    task: &DueTask,
    sent_at: &str,
    recipient: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT OR IGNORE INTO email_notification_logs (
              entity_type, entity_id, due_date, reminder_type, sent_at, recipient
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                task.entity_type,
                task.entity_id,
                task.due_date,
                REMINDER_TYPE_DUE_TOMORROW_21,
                sent_at,
                recipient
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn build_due_task_email_body(due_date: &str, tasks: &[DueTask]) -> String {
    let mut lines = vec![format!("以下任务将在 {} 到期：", due_date), String::new()];
    for task in tasks {
        let source = if task.entity_type == "today_plan_item" {
            "今日任务"
        } else {
            "清单任务"
        };
        lines.push(format!("- [{}] {}", source, task.title));
    }
    lines.push(String::new());
    lines.push("来自考研专注。".to_string());
    lines.join("\n")
}

fn is_after_reminder_time() -> bool {
    let now = Local::now();
    now.hour() >= 21
}

fn skipped(message: &str) -> EmailReminderResult {
    EmailReminderResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        sent_count: 0,
    }
}

fn get_string_setting(
    connection: &Connection,
    key: &str,
    fallback: &str,
) -> Result<String, String> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| fallback.to_string()))
}

fn get_i64_setting(connection: &Connection, key: &str, fallback: i64) -> Result<i64, String> {
    let raw = get_string_setting(connection, key, &fallback.to_string())?;
    Ok(raw.parse::<i64>().unwrap_or(fallback))
}

fn set_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![key, value, updated_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

use crate::{
    credential, runtime_health, storage::db::open_database, sync_package::mark_entity_deleted,
};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use reqwest::{
    blocking::{Client, Response},
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Method, StatusCode, Url,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    thread,
};
use tauri::{AppHandle, Manager};

const ENTITY_SCHEDULE_BLOCK: &str = "schedule_block";
const PROVIDER_CALDAV: &str = "caldav";
const CALDAV_ENABLED_KEY: &str = "caldav_enabled";
const CALDAV_SERVER_URL_KEY: &str = "caldav_server_url";
const CALDAV_USERNAME_KEY: &str = "caldav_username";
const CALDAV_PASSWORD_KEY: &str = "caldav_password";
const CALDAV_SELECTED_CALENDAR_URL_KEY: &str = "caldav_selected_calendar_url";
const CALDAV_SELECTED_CALENDAR_NAME_KEY: &str = "caldav_selected_calendar_name";
const MARKER_PREFIX: &str = "[kaoyan-focus:";
const TIME_ZONE: &str = "Asia/Shanghai";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalDavSettings {
    pub enabled: bool,
    pub server_url: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub password_configured: bool,
    pub selected_calendar_url: String,
    pub selected_calendar_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalDavCalendar {
    pub url: String,
    pub name: String,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalDavStatus {
    pub configured: bool,
    pub calendar_url: String,
    pub calendar_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalDavSyncResult {
    pub status: String,
    pub message: String,
    pub pushed_count: i64,
    pub pulled_count: i64,
    pub deleted_count: i64,
    pub conflict_count: i64,
    pub event_count: i64,
    pub synced_at: String,
}

#[derive(Debug, Clone)]
struct LocalCalendarBlock {
    id: i64,
    sync_id: String,
    schedule_date: String,
    title: String,
    note: Option<String>,
    start_minute: i64,
    end_minute: i64,
    status: String,
    updated_at: String,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct ImportedCalendarBlock {
    sync_id: String,
}

#[derive(Debug, Clone)]
struct RemoteCalendarEvent {
    id: String,
    etag: Option<String>,
    title: String,
    note: Option<String>,
    schedule_date: String,
    start_minute: i64,
    end_minute: i64,
    updated_millis: Option<i64>,
    marker_sync_id: Option<String>,
    raw_ics: String,
}

#[derive(Debug, Clone)]
struct CalendarSyncLink {
    id: i64,
    remote_id: String,
    remote_etag: Option<String>,
    remote_fingerprint: Option<String>,
    remote_last_modified: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkedCalendarAction {
    PushLocal,
    PullRemote,
    RefreshLink,
}

#[derive(Debug, Clone)]
struct SyncCounters {
    pushed_count: i64,
    pulled_count: i64,
    deleted_count: i64,
    conflict_count: i64,
    event_count: i64,
}

#[tauri::command]
pub fn get_caldav_settings(app: AppHandle) -> Result<CalDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    read_caldav_settings(&connection, false)
}

#[tauri::command]
pub fn save_caldav_settings(
    app: AppHandle,
    settings: CalDavSettings,
) -> Result<CalDavSettings, String> {
    let connection = open_database(&database_path(&app)?)?;
    let password_changed = !settings.password.is_empty();
    let normalized = normalize_settings(resolve_caldav_secret(&connection, settings)?)?;
    persist_caldav_settings(&connection, &normalized, password_changed)?;
    Ok(redact_settings(normalized))
}

#[tauri::command]
pub async fn discover_caldav_calendars(
    app: AppHandle,
    settings: CalDavSettings,
) -> Result<Vec<CalDavCalendar>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database(&database_path(&app)?)?;
        let password_changed = !settings.password.is_empty();
        let normalized = normalize_settings(resolve_caldav_secret(&connection, settings)?)?;
        persist_caldav_settings(&connection, &normalized, password_changed)?;
        discover_calendars_blocking(&normalized)
    })
    .await
    .map_err(|error| format!("CalDAV 日历发现后台任务失败：{error}"))?
}

#[tauri::command]
pub async fn test_caldav_connection(
    app: AppHandle,
    settings: CalDavSettings,
) -> Result<CalDavStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database(&database_path(&app)?)?;
        let password_changed = !settings.password.is_empty();
        let normalized = normalize_settings(resolve_caldav_secret(&connection, settings)?)?;
        persist_caldav_settings(&connection, &normalized, password_changed)?;
        let calendar_url = selected_calendar_url(&normalized)?;
        let client = caldav_client()?;
        let response = caldav_request(&client, Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?, &calendar_url, &normalized)
            .header("Depth", "0")
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:displayname/></d:prop></d:propfind>"#)
            .send()
            .map_err(|error| format!("CalDAV 连接失败：{error}"))?;
        let status = response.status();
        if status == StatusCode::OK || status.as_u16() == 207 {
            return Ok(CalDavStatus {
                configured: true,
                calendar_url,
                calendar_name: normalized.selected_calendar_name,
                message: "CalDAV 日历连接成功。".to_string(),
            });
        }
        Err(format!("CalDAV 连接失败，HTTP 状态：{}", status.as_u16()))
    })
    .await
    .map_err(|error| format!("CalDAV 连接测试后台任务失败：{error}"))?
}

#[tauri::command]
pub async fn sync_caldav_calendar(
    app: AppHandle,
    trigger: Option<String>,
) -> Result<CalDavSyncResult, String> {
    let trigger = trigger.unwrap_or_else(|| "manual".to_string());
    let database_path = database_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        sync_caldav_calendar_blocking(database_path, trigger)
    })
    .await
    .map_err(|error| format!("CalDAV 同步后台任务失败：{error}"))?
}

pub(crate) fn sync_caldav_calendar_after_local_change(app: AppHandle, trigger: &'static str) {
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(500));
        match database_path(&app)
            .and_then(|path| sync_caldav_calendar_blocking(path, trigger.to_string()).map(|_| ()))
        {
            Ok(()) => runtime_health::mark_task_success("caldav_background_sync", Some(60)),
            Err(error) => {
                runtime_health::mark_task_error("caldav_background_sync", &error, Some(120))
            }
        }
    });
}

fn sync_caldav_calendar_blocking(
    database_path: std::path::PathBuf,
    trigger: String,
) -> Result<CalDavSyncResult, String> {
    let mut connection = open_database(&database_path)?;
    let settings = read_caldav_settings(&connection, true)?;
    if !settings.enabled {
        return Ok(skipped_sync("CalDAV 已关闭，跳过同步。"));
    }
    if !settings_configured(&settings) {
        return Ok(skipped_sync("CalDAV 配置不完整，跳过同步。"));
    }

    let prefer_local_changes = trigger.ends_with("_change") || trigger.contains("local");
    let calendar_url = selected_calendar_url(&settings)?;
    let client = caldav_client()?;
    let remote_events = fetch_remote_events(&client, &calendar_url, &settings)?;
    let mut counters = SyncCounters {
        pushed_count: 0,
        pulled_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        event_count: remote_events.len() as i64,
    };
    sync_calendar_events(
        &mut connection,
        &client,
        &settings,
        &calendar_url,
        remote_events,
        prefer_local_changes,
        &mut counters,
    )?;
    Ok(CalDavSyncResult {
        status: "synced".to_string(),
        message: "CalDAV 日历同步完成。".to_string(),
        pushed_count: counters.pushed_count,
        pulled_count: counters.pulled_count,
        deleted_count: counters.deleted_count,
        conflict_count: counters.conflict_count,
        event_count: counters.event_count,
        synced_at: Utc::now().to_rfc3339(),
    })
}

fn skipped_sync(message: &str) -> CalDavSyncResult {
    CalDavSyncResult {
        status: "skipped".to_string(),
        message: message.to_string(),
        pushed_count: 0,
        pulled_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        event_count: 0,
        synced_at: Utc::now().to_rfc3339(),
    }
}

fn sync_calendar_events(
    connection: &mut Connection,
    client: &Client,
    settings: &CalDavSettings,
    calendar_url: &str,
    remote_events: Vec<RemoteCalendarEvent>,
    prefer_local_changes: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let remote_by_id = remote_events
        .iter()
        .map(|event| (event.id.clone(), event.clone()))
        .collect::<HashMap<_, _>>();
    let remote_by_marker = remote_events
        .iter()
        .filter_map(|event| {
            event
                .marker_sync_id
                .clone()
                .map(|sync_id| (sync_id, event.clone()))
        })
        .collect::<HashMap<_, _>>();

    for block in load_local_calendar_blocks(connection)? {
        if let Some(link) = get_calendar_link_by_sync_id(connection, &block.sync_id)? {
            if block.deleted_at.is_some() {
                delete_remote_event_if_present(client, settings, &link.remote_id)?;
                mark_calendar_link_deleted(connection, link.id)?;
                counters.deleted_count += 1;
                continue;
            }
            if let Some(remote) = remote_by_id.get(&link.remote_id) {
                sync_linked_event(
                    connection,
                    client,
                    settings,
                    calendar_url,
                    &block,
                    remote,
                    &link,
                    prefer_local_changes,
                    counters,
                )?;
            } else if date_in_sync_range(&block.schedule_date)
                && remote_event_deleted(client, settings, &link.remote_id)?
            {
                delete_local_schedule_block(connection, block.id)?;
                mark_calendar_link_deleted(connection, link.id)?;
                counters.deleted_count += 1;
            }
        } else if block.deleted_at.is_none() && date_in_sync_range(&block.schedule_date) {
            if let Some(remote) = remote_by_marker.get(&block.sync_id) {
                upsert_calendar_link(
                    connection,
                    Some(block.id),
                    &block.sync_id,
                    &remote.id,
                    Some(calendar_url),
                    remote.etag.as_deref(),
                    Some(&remote_event_fingerprint(remote)),
                    remote
                        .updated_millis
                        .map(|value| value.to_string())
                        .as_deref(),
                )?;
                continue;
            }
            let remote_id = event_url_for_sync_id(calendar_url, &block.sync_id)?;
            put_remote_event(client, settings, &remote_id, &block, None)?;
            let local_fingerprint = local_calendar_block_fingerprint(&block);
            upsert_calendar_link(
                connection,
                Some(block.id),
                &block.sync_id,
                &remote_id,
                Some(calendar_url),
                None,
                Some(&local_fingerprint),
                None,
            )?;
            counters.pushed_count += 1;
        }
    }

    for remote in remote_events {
        if get_calendar_link_by_remote_id(connection, &remote.id)?.is_some() {
            continue;
        }
        if let Some(sync_id) = remote.marker_sync_id.as_deref() {
            if let Some((local_id, deleted_at)) =
                get_sync_meta_local_by_sync_id(connection, ENTITY_SCHEDULE_BLOCK, sync_id)?
            {
                if deleted_at.is_none() {
                    upsert_calendar_link(
                        connection,
                        Some(local_id),
                        sync_id,
                        &remote.id,
                        Some(calendar_url),
                        remote.etag.as_deref(),
                        Some(&remote_event_fingerprint(&remote)),
                        remote
                            .updated_millis
                            .map(|value| value.to_string())
                            .as_deref(),
                    )?;
                }
                continue;
            }
        }
        if is_importable_remote_event(&remote) {
            let block = create_local_schedule_block_from_remote(connection, &remote)?;
            if remote.marker_sync_id.is_none() {
                if let Ok(updated_ics) =
                    mark_remote_ics_with_sync_marker(&remote.raw_ics, &block.sync_id)
                {
                    let _ = put_remote_ics(
                        client,
                        settings,
                        &remote.id,
                        updated_ics,
                        remote.etag.as_deref(),
                    );
                }
            }
            counters.pulled_count += 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sync_linked_event(
    connection: &Connection,
    client: &Client,
    settings: &CalDavSettings,
    calendar_url: &str,
    local: &LocalCalendarBlock,
    remote: &RemoteCalendarEvent,
    link: &CalendarSyncLink,
    prefer_local_changes: bool,
    counters: &mut SyncCounters,
) -> Result<(), String> {
    let local_updated = parse_rfc3339_millis(&local.updated_at)?;
    let remote_updated = remote.updated_millis.or_else(|| {
        link.remote_last_modified
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
    });
    let local_fingerprint = local_calendar_block_fingerprint(local);
    let remote_fingerprint = remote_event_fingerprint(remote);
    let local_changed_since_sync = link
        .remote_fingerprint
        .as_deref()
        .map(|fingerprint| fingerprint != local_fingerprint)
        .unwrap_or(local_fingerprint != remote_fingerprint);
    let remote_changed_since_sync = link
        .remote_fingerprint
        .as_deref()
        .map(|fingerprint| fingerprint != remote_fingerprint)
        .unwrap_or(false)
        || link
            .remote_etag
            .as_deref()
            .zip(remote.etag.as_deref())
            .is_some_and(|(old, new)| old != new);

    match linked_calendar_action(
        local_updated,
        remote_updated,
        local_changed_since_sync,
        remote_changed_since_sync,
        prefer_local_changes,
    ) {
        LinkedCalendarAction::PushLocal => {
            put_existing_remote_event(client, settings, remote, local)?;
            upsert_calendar_link(
                connection,
                Some(local.id),
                &local.sync_id,
                &remote.id,
                Some(calendar_url),
                remote.etag.as_deref(),
                Some(&local_calendar_block_fingerprint(local)),
                remote_updated.map(|value| value.to_string()).as_deref(),
            )?;
            counters.pushed_count += 1;
        }
        LinkedCalendarAction::PullRemote => {
            update_local_schedule_block_from_remote(connection, local.id, remote)?;
            upsert_calendar_link(
                connection,
                Some(local.id),
                &local.sync_id,
                &remote.id,
                Some(calendar_url),
                remote.etag.as_deref(),
                Some(&remote_event_fingerprint(remote)),
                remote_updated.map(|value| value.to_string()).as_deref(),
            )?;
            counters.pulled_count += 1;
        }
        LinkedCalendarAction::RefreshLink => {
            upsert_calendar_link(
                connection,
                Some(local.id),
                &local.sync_id,
                &remote.id,
                Some(calendar_url),
                remote.etag.as_deref(),
                Some(&local_calendar_block_fingerprint(local)),
                remote_updated.map(|value| value.to_string()).as_deref(),
            )?;
        }
    }
    Ok(())
}

fn linked_calendar_action(
    local_updated: i64,
    remote_updated: Option<i64>,
    local_changed_since_sync: bool,
    remote_changed_since_sync: bool,
    prefer_local_changes: bool,
) -> LinkedCalendarAction {
    const SKEW_MILLIS: i64 = 1_000;
    match (local_changed_since_sync, remote_changed_since_sync) {
        (true, false) => LinkedCalendarAction::PushLocal,
        (false, true) => LinkedCalendarAction::PullRemote,
        (false, false) => LinkedCalendarAction::RefreshLink,
        (true, true) => {
            if let Some(remote_updated) = remote_updated {
                if local_updated > remote_updated + SKEW_MILLIS {
                    return LinkedCalendarAction::PushLocal;
                }
                if remote_updated > local_updated + SKEW_MILLIS {
                    return LinkedCalendarAction::PullRemote;
                }
            }
            if prefer_local_changes {
                LinkedCalendarAction::PushLocal
            } else {
                LinkedCalendarAction::PullRemote
            }
        }
    }
}

fn discover_calendars_blocking(settings: &CalDavSettings) -> Result<Vec<CalDavCalendar>, String> {
    let client = caldav_client()?;
    let base = normalized_server_url(&settings.server_url)?;
    let discovery_url = well_known_caldav_url(&base)?;
    let principal_url = current_user_principal(&client, &discovery_url, settings)
        .or_else(|_| current_user_principal(&client, &base, settings))?;
    let home_set = calendar_home_set(&client, &principal_url, settings)?;
    let response = propfind_calendars(&client, &home_set, settings)?;
    parse_calendar_discovery_response(&home_set, &response)
}

fn current_user_principal(
    client: &Client,
    url: &str,
    settings: &CalDavSettings,
) -> Result<String, String> {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
    <d:propfind xmlns:d="DAV:">
      <d:prop><d:current-user-principal/></d:prop>
    </d:propfind>"#;
    let text = send_propfind(client, url, settings, "0", body)?;
    let href = first_href_for_property(&text, "current-user-principal")
        .ok_or_else(|| "CalDAV 服务器未返回 current-user-principal。".to_string())?;
    resolve_url(url, &href)
}

fn calendar_home_set(
    client: &Client,
    principal_url: &str,
    settings: &CalDavSettings,
) -> Result<String, String> {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
    <d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
      <d:prop><cal:calendar-home-set/></d:prop>
    </d:propfind>"#;
    let text = send_propfind(client, principal_url, settings, "0", body)?;
    let href = first_href_for_property(&text, "calendar-home-set")
        .ok_or_else(|| "CalDAV 服务器未返回 calendar-home-set。".to_string())?;
    resolve_url(principal_url, &href)
}

fn propfind_calendars(
    client: &Client,
    home_set_url: &str,
    settings: &CalDavSettings,
) -> Result<String, String> {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
    <d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
      <d:prop>
        <d:displayname/>
        <d:resourcetype/>
        <d:current-user-privilege-set/>
        <cal:supported-calendar-component-set/>
      </d:prop>
    </d:propfind>"#;
    send_propfind(client, home_set_url, settings, "1", body)
}

fn send_propfind(
    client: &Client,
    url: &str,
    settings: &CalDavSettings,
    depth: &str,
    body: &str,
) -> Result<String, String> {
    let response = caldav_request(
        client,
        Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?,
        url,
        settings,
    )
    .header("Depth", depth)
    .header(CONTENT_TYPE, "application/xml; charset=utf-8")
    .body(body.to_string())
    .send()
    .map_err(|error| format!("CalDAV PROPFIND 失败：{error}"))?;
    response_text(response)
}

fn fetch_remote_events(
    client: &Client,
    calendar_url: &str,
    settings: &CalDavSettings,
) -> Result<Vec<RemoteCalendarEvent>, String> {
    let (start, end) = calendar_sync_range_utc_strings();
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
        <cal:calendar-query xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
          <d:prop><d:getetag/><cal:calendar-data/></d:prop>
          <cal:filter>
            <cal:comp-filter name="VCALENDAR">
              <cal:comp-filter name="VEVENT">
                <cal:time-range start="{start}" end="{end}"/>
              </cal:comp-filter>
            </cal:comp-filter>
          </cal:filter>
        </cal:calendar-query>"#
    );
    let response = caldav_request(
        client,
        Method::from_bytes(b"REPORT").map_err(|error| error.to_string())?,
        calendar_url,
        settings,
    )
    .header("Depth", "1")
    .header(CONTENT_TYPE, "application/xml; charset=utf-8")
    .body(body)
    .send()
    .map_err(|error| format!("CalDAV REPORT 失败：{error}"))?;
    let text = response_text(response)?;
    parse_calendar_query_response(calendar_url, &text)
}

fn put_remote_event(
    client: &Client,
    settings: &CalDavSettings,
    event_url: &str,
    block: &LocalCalendarBlock,
    etag: Option<&str>,
) -> Result<(), String> {
    let ics = build_ics_event(block, TIME_ZONE)?;
    put_remote_ics(client, settings, event_url, ics, etag)
}

fn put_existing_remote_event(
    client: &Client,
    settings: &CalDavSettings,
    remote: &RemoteCalendarEvent,
    block: &LocalCalendarBlock,
) -> Result<(), String> {
    let ics = build_ics_event_from_existing_remote(&remote.raw_ics, block, TIME_ZONE)?;
    put_remote_ics(client, settings, &remote.id, ics, remote.etag.as_deref())
}

fn put_remote_ics(
    client: &Client,
    settings: &CalDavSettings,
    event_url: &str,
    ics: String,
    etag: Option<&str>,
) -> Result<(), String> {
    let mut request = caldav_request(client, Method::PUT, event_url, settings)
        .header(CONTENT_TYPE, "text/calendar; charset=utf-8")
        .body(ics);
    if let Some(etag) = etag {
        request = request.header("If-Match", etag);
    }
    let response = request
        .send()
        .map_err(|error| format!("CalDAV PUT 失败：{error}"))?;
    if response.status().is_success()
        || response.status() == StatusCode::CREATED
        || response.status() == StatusCode::NO_CONTENT
    {
        return Ok(());
    }
    Err(format!(
        "CalDAV PUT 失败，HTTP 状态：{}",
        response.status().as_u16()
    ))
}

fn delete_remote_event_if_present(
    client: &Client,
    settings: &CalDavSettings,
    event_url: &str,
) -> Result<(), String> {
    let response = caldav_request(client, Method::DELETE, event_url, settings)
        .send()
        .map_err(|error| format!("CalDAV DELETE 失败：{error}"))?;
    if response.status().is_success()
        || response.status() == StatusCode::NOT_FOUND
        || response.status() == StatusCode::NO_CONTENT
    {
        return Ok(());
    }
    Err(format!(
        "CalDAV DELETE 失败，HTTP 状态：{}",
        response.status().as_u16()
    ))
}

fn remote_event_deleted(
    client: &Client,
    settings: &CalDavSettings,
    event_url: &str,
) -> Result<bool, String> {
    let response = caldav_request(client, Method::GET, event_url, settings)
        .send()
        .map_err(|_| "probe_failed".to_string())?;
    Ok(response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::GONE)
}

fn caldav_request(
    client: &Client,
    method: Method,
    url: &str,
    settings: &CalDavSettings,
) -> reqwest::blocking::RequestBuilder {
    let mut headers = HeaderMap::new();
    if let Ok(value) =
        HeaderValue::from_str(&basic_auth_header(&settings.username, &settings.password))
    {
        headers.insert(AUTHORIZATION, value);
    }
    client.request(method, url).headers(headers)
}

fn response_text(response: Response) -> Result<String, String> {
    let status = response.status();
    if status == StatusCode::OK || status.as_u16() == 207 {
        return response.text().map_err(|error| error.to_string());
    }
    Err(format!("CalDAV 请求失败，HTTP 状态：{}", status.as_u16()))
}

fn parse_calendar_discovery_response(
    base_url: &str,
    xml: &str,
) -> Result<Vec<CalDavCalendar>, String> {
    let mut calendars = Vec::new();
    for response in parse_multistatus_responses(xml)? {
        if !response.has_success() {
            continue;
        }
        if !response.has_tag("calendar") {
            continue;
        }
        if !response.supports_vevent() {
            continue;
        }
        let Some(href) = response.first_text("href") else {
            continue;
        };
        let url = resolve_url(base_url, &href)?;
        let name = response
            .first_text("displayname")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| url.clone());
        let writable = response.is_writable_calendar();
        if !writable {
            continue;
        }
        calendars.push(CalDavCalendar {
            url,
            name,
            writable,
        });
    }
    Ok(calendars)
}

fn parse_calendar_query_response(
    base_url: &str,
    xml: &str,
) -> Result<Vec<RemoteCalendarEvent>, String> {
    let mut events = Vec::new();
    for response in parse_multistatus_responses(xml)? {
        if !response.has_success() {
            continue;
        }
        let Some(href) = response.first_text("href") else {
            continue;
        };
        let Some(calendar_data) = response.first_text("calendar-data") else {
            continue;
        };
        let event_url = resolve_url(base_url, &href)?;
        let etag = response.first_text("getetag");
        if let Some(event) = parse_ics_event(&event_url, etag.as_deref(), &calendar_data)? {
            events.push(event);
        }
    }
    Ok(events)
}

fn parse_ics_event(
    event_url: &str,
    etag: Option<&str>,
    ics: &str,
) -> Result<Option<RemoteCalendarEvent>, String> {
    let unfolded = unfold_ics_lines(ics);
    let mut in_event = false;
    let mut fields = HashMap::<String, String>::new();
    for line in unfolded {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            in_event = true;
            continue;
        }
        if line == "END:VEVENT" {
            break;
        }
        if !in_event {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let key = name
            .split(';')
            .next()
            .unwrap_or(name)
            .trim()
            .to_ascii_uppercase();
        fields.insert(key, unescape_ics_text(value));
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let title = fields.get("SUMMARY").cloned().unwrap_or_default();
    let description = fields.get("DESCRIPTION").cloned().unwrap_or_default();
    let marker_sync_id = fields
        .get("X-KAOYAN-FOCUS-SYNC-ID")
        .cloned()
        .or_else(|| extract_marker(&description));
    let note = Some(strip_marker(&description)).filter(|value| !value.trim().is_empty());
    let start_raw = fields
        .get("DTSTART")
        .ok_or_else(|| "VEVENT 缺少 DTSTART。".to_string())?;
    let end_raw = fields
        .get("DTEND")
        .ok_or_else(|| "VEVENT 缺少 DTEND。".to_string())?;
    let (schedule_date, start_minute) = parse_ics_date_time(start_raw)?;
    let (end_date, mut end_minute) = parse_ics_date_time(end_raw)?;
    if end_date != schedule_date {
        end_minute = 1440;
    }
    if end_minute <= start_minute {
        end_minute = (start_minute + 60).min(1440);
    }
    let updated_millis = fields
        .get("LAST-MODIFIED")
        .or_else(|| fields.get("DTSTAMP"))
        .and_then(|value| parse_ics_date_time_utc(value).ok())
        .map(|value| value.timestamp_millis());
    Ok(Some(RemoteCalendarEvent {
        id: event_url.to_string(),
        etag: etag.map(str::to_string),
        title,
        note,
        schedule_date,
        start_minute,
        end_minute,
        updated_millis,
        marker_sync_id,
        raw_ics: ics.to_string(),
    }))
}

fn build_ics_event(block: &LocalCalendarBlock, _timezone: &str) -> Result<String, String> {
    let start = local_date_minute_to_ics(&block.schedule_date, block.start_minute)?;
    let end = local_date_minute_to_ics(&block.schedule_date, block.end_minute)?;
    let dtstamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let description = body_with_marker(block.note.as_deref(), &block.sync_id);
    Ok([
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//kaoyan-focus//calendar//ZH-CN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}@kaoyan-focus", escape_ics_text(&block.sync_id)),
        format!("DTSTAMP:{dtstamp}"),
        format!("DTSTART:{start}"),
        format!("DTEND:{end}"),
        format!("SUMMARY:{}", escape_ics_text(&block.title)),
        format!("DESCRIPTION:{}", escape_ics_text(&description)),
        format!("X-KAOYAN-FOCUS-SYNC-ID:{}", escape_ics_text(&block.sync_id)),
        format!(
            "STATUS:{}",
            if block.status == "completed" {
                "CONFIRMED"
            } else {
                "TENTATIVE"
            }
        ),
        "END:VEVENT".to_string(),
        "END:VCALENDAR".to_string(),
        String::new(),
    ]
    .join("\r\n"))
}

fn build_ics_event_from_existing_remote(
    raw_ics: &str,
    block: &LocalCalendarBlock,
    _timezone: &str,
) -> Result<String, String> {
    let start = local_date_minute_to_ics(&block.schedule_date, block.start_minute)?;
    let end = local_date_minute_to_ics(&block.schedule_date, block.end_minute)?;
    let dtstamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let description = body_with_marker(block.note.as_deref(), &block.sync_id);
    rewrite_first_vevent_properties(
        raw_ics,
        &[
            ("DTSTAMP", dtstamp),
            ("DTSTART", start),
            ("DTEND", end),
            ("SUMMARY", escape_ics_text(&block.title)),
            ("DESCRIPTION", escape_ics_text(&description)),
            ("X-KAOYAN-FOCUS-SYNC-ID", escape_ics_text(&block.sync_id)),
            (
                "STATUS",
                if block.status == "completed" {
                    "CONFIRMED".to_string()
                } else {
                    "TENTATIVE".to_string()
                },
            ),
        ],
    )
}

fn mark_remote_ics_with_sync_marker(raw_ics: &str, sync_id: &str) -> Result<String, String> {
    rewrite_first_vevent_properties(
        raw_ics,
        &[("X-KAOYAN-FOCUS-SYNC-ID", escape_ics_text(sync_id))],
    )
}

fn rewrite_first_vevent_properties(
    raw_ics: &str,
    properties: &[(&str, String)],
) -> Result<String, String> {
    let lines = split_ics_lines(raw_ics);
    let replace_names = properties
        .iter()
        .map(|(name, _)| name.to_ascii_uppercase())
        .collect::<HashSet<_>>();
    let mut result = Vec::<String>::new();
    let mut in_event = false;
    let mut event_done = false;
    let mut skipping_replaced_property = false;
    let mut saw_uid = false;
    let mut inserted = false;

    for line in lines {
        if skipping_replaced_property && is_ics_continuation_line(&line) {
            continue;
        }
        skipping_replaced_property = false;

        if !event_done && line.trim_end_matches('\r') == "BEGIN:VEVENT" {
            in_event = true;
            result.push(line);
            continue;
        }

        if in_event && line.trim_end_matches('\r') == "END:VEVENT" {
            for (name, value) in properties {
                result.push(format!("{}:{}", name.to_ascii_uppercase(), value));
            }
            inserted = true;
            in_event = false;
            event_done = true;
            result.push(line);
            continue;
        }

        if in_event {
            if let Some(name) = ics_property_name(&line) {
                if name == "UID" {
                    saw_uid = true;
                }
                if replace_names.contains(&name) {
                    skipping_replaced_property = true;
                    continue;
                }
            }
        }

        result.push(line);
    }

    if !inserted {
        return Err("VEVENT 缺少 END:VEVENT，无法安全写入 CalDAV 同步标记。".to_string());
    }
    if !saw_uid {
        return Err("VEVENT 缺少 UID，拒绝覆盖远端事件以避免产生重复日程。".to_string());
    }

    Ok(normalize_ics_lines(result))
}

fn create_local_schedule_block_from_remote(
    connection: &Connection,
    remote: &RemoteCalendarEvent,
) -> Result<ImportedCalendarBlock, String> {
    let now = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    connection
        .execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              start_minute, end_minute, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 'general', NULL, NULL, ?4, ?5, 'planned', ?6, ?6)
            ",
            params![
                remote.schedule_date,
                remote.title.trim(),
                remote.note,
                remote.start_minute,
                remote.end_minute,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    let sync_id = remote
        .marker_sync_id
        .clone()
        .unwrap_or_else(|| format!("{ENTITY_SCHEDULE_BLOCK}-{local_id}"));
    ensure_sync_meta(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        local_id,
        &sync_id,
        &now,
        None,
    )?;
    upsert_calendar_link(
        connection,
        Some(local_id),
        &sync_id,
        &remote.id,
        None,
        remote.etag.as_deref(),
        Some(&remote_event_fingerprint(remote)),
        remote
            .updated_millis
            .map(|value| value.to_string())
            .as_deref(),
    )?;
    Ok(ImportedCalendarBlock { sync_id })
}

fn update_local_schedule_block_from_remote(
    connection: &Connection,
    id: i64,
    remote: &RemoteCalendarEvent,
) -> Result<(), String> {
    let updated_at = remote
        .updated_millis
        .map(millis_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    connection
        .execute(
            "
            UPDATE schedule_blocks
            SET schedule_date = ?1,
                title = CASE WHEN ?2 = '' THEN title ELSE ?2 END,
                note = ?3,
                start_minute = ?4,
                end_minute = ?5,
                updated_at = ?6
            WHERE id = ?7
            ",
            params![
                remote.schedule_date,
                remote.title.trim(),
                remote.note,
                remote.start_minute,
                remote.end_minute,
                updated_at,
                id
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn delete_local_schedule_block(connection: &Connection, id: i64) -> Result<(), String> {
    mark_entity_deleted(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        id,
        Utc::now().timestamp_millis(),
    )?;
    connection
        .execute("DELETE FROM schedule_blocks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_local_calendar_blocks(connection: &Connection) -> Result<Vec<LocalCalendarBlock>, String> {
    let mut blocks = Vec::new();
    let mut statement = connection
        .prepare(
            "
            SELECT b.id, b.schedule_date, b.title, b.note, b.start_minute, b.end_minute,
                   b.status, b.updated_at, m.sync_id, m.deleted_at
            FROM schedule_blocks b
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
            ORDER BY b.schedule_date ASC, b.start_minute ASC, b.id ASC
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            Ok(LocalCalendarBlock {
                id,
                schedule_date: row.get(1)?,
                title: row.get(2)?,
                note: row.get(3)?,
                start_minute: row.get(4)?,
                end_minute: row.get(5)?,
                status: row.get(6)?,
                updated_at: row.get(7)?,
                sync_id: row
                    .get::<_, Option<String>>(8)?
                    .unwrap_or_else(|| format!("{ENTITY_SCHEDULE_BLOCK}-{id}")),
                deleted_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let block = row.map_err(|error| error.to_string())?;
        ensure_sync_meta(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            block.id,
            &block.sync_id,
            &block.updated_at,
            None,
        )?;
        blocks.push(block);
    }
    blocks.extend(load_tombstone_blocks(connection)?);
    Ok(blocks)
}

fn load_tombstone_blocks(connection: &Connection) -> Result<Vec<LocalCalendarBlock>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT local_id, sync_id, deleted_at, updated_at
            FROM sync_meta
            WHERE entity_type = 'schedule_block' AND deleted_at IS NOT NULL
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let local_id: i64 = row.get(0)?;
            Ok(LocalCalendarBlock {
                id: local_id,
                sync_id: row.get(1)?,
                schedule_date: Local::now().date_naive().format("%Y-%m-%d").to_string(),
                title: String::new(),
                note: None,
                start_minute: 0,
                end_minute: 60,
                status: "planned".to_string(),
                updated_at: millis_to_rfc3339(row.get(3)?),
                deleted_at: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn get_calendar_link_by_sync_id(
    connection: &Connection,
    sync_id: &str,
) -> Result<Option<CalendarSyncLink>, String> {
    connection
        .query_row(
            "
            SELECT id, remote_id, remote_etag, remote_fingerprint, remote_last_modified
            FROM calendar_sync_links
            WHERE entity_type = ?1 AND local_sync_id = ?2 AND provider = ?3 AND deleted_at IS NULL
            ",
            params![ENTITY_SCHEDULE_BLOCK, sync_id, PROVIDER_CALDAV],
            row_to_calendar_link,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_calendar_link_by_remote_id(
    connection: &Connection,
    remote_id: &str,
) -> Result<Option<CalendarSyncLink>, String> {
    connection
        .query_row(
            "
            SELECT id, remote_id, remote_etag, remote_fingerprint, remote_last_modified
            FROM calendar_sync_links
            WHERE provider = ?1 AND remote_id = ?2 AND deleted_at IS NULL
            ",
            params![PROVIDER_CALDAV, remote_id],
            row_to_calendar_link,
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn upsert_calendar_link(
    connection: &Connection,
    local_id: Option<i64>,
    local_sync_id: &str,
    remote_id: &str,
    remote_parent_id: Option<&str>,
    remote_etag: Option<&str>,
    remote_fingerprint: Option<&str>,
    remote_last_modified: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            DELETE FROM calendar_sync_links
            WHERE provider = ?1
              AND remote_id = ?2
              AND NOT (entity_type = ?3 AND local_sync_id = ?4 AND provider = ?1)
            ",
            params![
                PROVIDER_CALDAV,
                remote_id,
                ENTITY_SCHEDULE_BLOCK,
                local_sync_id
            ],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "
            INSERT INTO calendar_sync_links (
              entity_type, local_id, local_sync_id, provider, remote_id, remote_parent_id,
              remote_etag, remote_fingerprint, remote_last_modified, last_synced_at, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
            ON CONFLICT(entity_type, local_sync_id, provider) DO UPDATE SET
              local_id = excluded.local_id,
              remote_id = excluded.remote_id,
              remote_parent_id = excluded.remote_parent_id,
              remote_etag = excluded.remote_etag,
              remote_fingerprint = excluded.remote_fingerprint,
              remote_last_modified = COALESCE(excluded.remote_last_modified, calendar_sync_links.remote_last_modified),
              last_synced_at = excluded.last_synced_at,
              deleted_at = NULL
            ",
            params![
                ENTITY_SCHEDULE_BLOCK,
                local_id,
                local_sync_id,
                PROVIDER_CALDAV,
                remote_id,
                remote_parent_id,
                remote_etag,
                remote_fingerprint,
                remote_last_modified,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn mark_calendar_link_deleted(connection: &Connection, link_id: i64) -> Result<(), String> {
    connection
        .execute(
            "UPDATE calendar_sync_links SET deleted_at = ?1, last_synced_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), link_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn row_to_calendar_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarSyncLink> {
    Ok(CalendarSyncLink {
        id: row.get(0)?,
        remote_id: row.get(1)?,
        remote_etag: row.get(2)?,
        remote_fingerprint: row.get(3)?,
        remote_last_modified: row.get(4)?,
    })
}

fn ensure_sync_meta(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    sync_id: &str,
    updated_at: &str,
    deleted_at: Option<i64>,
) -> Result<(), String> {
    let updated_at_millis = parse_rfc3339_millis(updated_at)?;
    connection
        .execute(
            "
            INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(entity_type, local_id) DO UPDATE SET
              sync_id = excluded.sync_id,
              deleted_at = excluded.deleted_at,
              updated_at = excluded.updated_at
            ",
            params![
                entity_type,
                local_id,
                sync_id,
                deleted_at,
                updated_at_millis
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_sync_meta_local_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: &str,
) -> Result<Option<(i64, Option<i64>)>, String> {
    connection
        .query_row(
            "
            SELECT local_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND sync_id = ?2
            ",
            params![entity_type, sync_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn local_calendar_block_fingerprint(block: &LocalCalendarBlock) -> String {
    calendar_fingerprint(
        &block.schedule_date,
        block.start_minute,
        block.end_minute,
        &block.title,
        block.note.as_deref(),
    )
}

fn remote_event_fingerprint(remote: &RemoteCalendarEvent) -> String {
    calendar_fingerprint(
        &remote.schedule_date,
        remote.start_minute,
        remote.end_minute,
        &remote.title,
        remote.note.as_deref(),
    )
}

fn calendar_fingerprint(
    schedule_date: &str,
    start_minute: i64,
    end_minute: i64,
    title: &str,
    note: Option<&str>,
) -> String {
    [
        schedule_date.trim().to_string(),
        start_minute.to_string(),
        end_minute.to_string(),
        title.trim().to_string(),
        note.unwrap_or("").trim().to_string(),
    ]
    .join("\u{1f}")
}

fn is_importable_remote_event(remote: &RemoteCalendarEvent) -> bool {
    !remote.title.trim().is_empty() && date_in_sync_range(&remote.schedule_date)
}

fn read_caldav_settings(
    connection: &Connection,
    include_secret: bool,
) -> Result<CalDavSettings, String> {
    let password_configured = credential::secret_configured(connection, CALDAV_PASSWORD_KEY)?;
    Ok(CalDavSettings {
        enabled: get_bool_setting(connection, CALDAV_ENABLED_KEY, false)?,
        server_url: get_string_setting(connection, CALDAV_SERVER_URL_KEY, "")?,
        username: get_string_setting(connection, CALDAV_USERNAME_KEY, "")?,
        password: if include_secret {
            credential::get_secret(connection, CALDAV_PASSWORD_KEY)?
        } else {
            String::new()
        },
        password_configured,
        selected_calendar_url: get_string_setting(
            connection,
            CALDAV_SELECTED_CALENDAR_URL_KEY,
            "",
        )?,
        selected_calendar_name: get_string_setting(
            connection,
            CALDAV_SELECTED_CALENDAR_NAME_KEY,
            "",
        )?,
    })
}

fn resolve_caldav_secret(
    connection: &Connection,
    settings: CalDavSettings,
) -> Result<CalDavSettings, String> {
    if settings.password.is_empty() {
        Ok(CalDavSettings {
            password: credential::get_secret(connection, CALDAV_PASSWORD_KEY)?,
            ..settings
        })
    } else {
        Ok(settings)
    }
}

fn normalize_settings(settings: CalDavSettings) -> Result<CalDavSettings, String> {
    let server_url = normalize_url(&settings.server_url)?;
    let selected_calendar_url = if settings.selected_calendar_url.trim().is_empty() {
        String::new()
    } else {
        normalize_url(&settings.selected_calendar_url)?
    };
    let password_configured = settings.password_configured || !settings.password.is_empty();
    Ok(CalDavSettings {
        enabled: settings.enabled,
        server_url,
        username: settings.username.trim().to_string(),
        password: settings.password,
        password_configured,
        selected_calendar_url,
        selected_calendar_name: settings.selected_calendar_name.trim().to_string(),
    })
}

fn persist_caldav_settings(
    connection: &Connection,
    settings: &CalDavSettings,
    password_changed: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    set_setting(
        connection,
        CALDAV_ENABLED_KEY,
        if settings.enabled { "1" } else { "0" },
        &now,
    )?;
    set_setting(
        connection,
        CALDAV_SERVER_URL_KEY,
        &settings.server_url,
        &now,
    )?;
    set_setting(connection, CALDAV_USERNAME_KEY, &settings.username, &now)?;
    set_setting(
        connection,
        CALDAV_SELECTED_CALENDAR_URL_KEY,
        &settings.selected_calendar_url,
        &now,
    )?;
    set_setting(
        connection,
        CALDAV_SELECTED_CALENDAR_NAME_KEY,
        &settings.selected_calendar_name,
        &now,
    )?;
    if password_changed {
        credential::set_secret(connection, CALDAV_PASSWORD_KEY, &settings.password, &now)?;
    } else {
        credential::set_secret_if_changed(connection, CALDAV_PASSWORD_KEY, "", &now)?;
    }
    Ok(())
}

fn redact_settings(settings: CalDavSettings) -> CalDavSettings {
    CalDavSettings {
        password: String::new(),
        password_configured: settings.password_configured || !settings.password.is_empty(),
        ..settings
    }
}

fn settings_configured(settings: &CalDavSettings) -> bool {
    !settings.server_url.trim().is_empty()
        && !settings.username.trim().is_empty()
        && !settings.password.is_empty()
        && !settings.selected_calendar_url.trim().is_empty()
}

fn selected_calendar_url(settings: &CalDavSettings) -> Result<String, String> {
    if settings.selected_calendar_url.trim().is_empty() {
        Err("请先发现并选择一个 CalDAV 日历。".to_string())
    } else {
        normalize_url(&settings.selected_calendar_url)
    }
}

fn normalized_server_url(value: &str) -> Result<String, String> {
    normalize_url(value)
}

fn normalize_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed =
        Url::parse(trimmed).map_err(|_| "CalDAV 地址必须是完整的 http(s) URL。".to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("CalDAV 地址只支持 http 或 https。".to_string());
    }
    Ok(parsed.to_string())
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

fn get_bool_setting(connection: &Connection, key: &str, fallback: bool) -> Result<bool, String> {
    let raw = get_string_setting(connection, key, if fallback { "1" } else { "0" })?;
    Ok(matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
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

fn caldav_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| error.to_string())
}

fn basic_auth_header(username: &str, password: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

fn well_known_caldav_url(base: &str) -> Result<String, String> {
    let mut url = Url::parse(base).map_err(|error| error.to_string())?;
    url.set_path("/.well-known/caldav");
    url.set_query(None);
    Ok(url.to_string())
}

fn resolve_url(base_url: &str, href: &str) -> Result<String, String> {
    Url::parse(base_url)
        .map_err(|error| error.to_string())?
        .join(href.trim())
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

fn event_url_for_sync_id(calendar_url: &str, sync_id: &str) -> Result<String, String> {
    let safe = sync_id
        .chars()
        .map(|item| {
            if item.is_ascii_alphanumeric() || item == '-' || item == '_' {
                item
            } else {
                '-'
            }
        })
        .collect::<String>();
    Url::parse(calendar_url)
        .map_err(|error| error.to_string())?
        .join(&format!("{safe}.ics"))
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

#[derive(Debug, Default)]
struct CalDavXmlResponse {
    tags: Vec<CalDavXmlTag>,
    texts: Vec<CalDavXmlText>,
    has_vevent_support: bool,
}

#[derive(Debug)]
struct CalDavXmlTag {
    local_name: String,
}

#[derive(Debug)]
struct CalDavXmlText {
    local_name: String,
    ancestors: Vec<usize>,
    text: String,
}

#[derive(Debug)]
struct CalDavXmlStackEntry {
    local_name: String,
    tag_index: Option<usize>,
}

impl CalDavXmlResponse {
    fn first_text(&self, local_name: &str) -> Option<String> {
        self.texts
            .iter()
            .find(|entry| entry.local_name == local_name)
            .map(|entry| entry.text.trim().to_string())
    }

    fn has_tag(&self, local_name: &str) -> bool {
        self.tags.iter().any(|tag| tag.local_name == local_name)
    }

    fn has_success(&self) -> bool {
        self.texts
            .iter()
            .filter(|entry| entry.local_name == "status")
            .any(|entry| {
                entry
                    .text
                    .split_ascii_whitespace()
                    .any(|part| matches!(part, "200" | "201" | "204" | "207"))
            })
    }

    fn supports_vevent(&self) -> bool {
        !self.has_tag("supported-calendar-component-set") || self.has_vevent_support
    }

    fn is_writable_calendar(&self) -> bool {
        if !self.has_tag("current-user-privilege-set") {
            return true;
        }

        self.has_tag("write")
            || self.has_tag("write-content")
            || self.has_tag("bind")
            || self.has_tag("all")
    }
}

fn parse_multistatus_responses(xml: &str) -> Result<Vec<CalDavXmlResponse>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut responses = Vec::new();
    let mut current: Option<CalDavXmlResponse> = None;
    let mut stack: Vec<CalDavXmlStackEntry> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                push_xml_start(&mut current, &mut stack, &start, false);
            }
            Ok(Event::Empty(start)) => {
                push_xml_start(&mut current, &mut stack, &start, true);
            }
            Ok(Event::End(end)) => {
                let local_name = xml_local_name(end.name().as_ref());
                let was_response_end = local_name == "response";
                let _ = stack.pop();
                if was_response_end {
                    if let Some(response) = current.take() {
                        responses.push(response);
                    }
                }
            }
            Ok(Event::Text(text)) => {
                let decoded = text.decode().map_err(|error| error.to_string())?;
                append_xml_text(&mut current, &stack, decoded.into_owned());
            }
            Ok(Event::CData(text)) => {
                let decoded = text.decode().map_err(|error| error.to_string())?;
                append_xml_text(&mut current, &stack, decoded.into_owned());
            }
            Ok(Event::GeneralRef(reference)) => {
                let decoded = decode_xml_general_ref(&String::from_utf8_lossy(reference.as_ref()));
                append_xml_text(&mut current, &stack, decoded);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(responses)
}

fn push_xml_start(
    current: &mut Option<CalDavXmlResponse>,
    stack: &mut Vec<CalDavXmlStackEntry>,
    start: &BytesStart<'_>,
    is_empty: bool,
) {
    let local_name = xml_local_name(start.name().as_ref());
    if current.is_none() && local_name == "response" {
        *current = Some(CalDavXmlResponse::default());
    }

    let tag_index = current.as_mut().map(|response| {
        let index = response.tags.len();
        if local_name == "comp" && start_has_case_insensitive_attribute(start, "name", "VEVENT") {
            response.has_vevent_support = true;
        }
        response.tags.push(CalDavXmlTag {
            local_name: local_name.clone(),
        });
        index
    });

    if !is_empty {
        stack.push(CalDavXmlStackEntry {
            local_name,
            tag_index,
        });
    }
}

fn append_xml_text(
    current: &mut Option<CalDavXmlResponse>,
    stack: &[CalDavXmlStackEntry],
    text: String,
) {
    if text.is_empty() {
        return;
    }
    let Some(response) = current.as_mut() else {
        return;
    };
    if stack.last().and_then(|entry| entry.tag_index).is_none() {
        return;
    }
    let local_name = stack
        .last()
        .map(|entry| entry.local_name.clone())
        .unwrap_or_default();
    let ancestors = stack
        .iter()
        .filter_map(|entry| entry.tag_index)
        .collect::<Vec<_>>();

    if let Some(last) = response.texts.last_mut() {
        if last.local_name == local_name && last.ancestors == ancestors {
            last.text.push_str(&text);
            return;
        }
    }

    response.texts.push(CalDavXmlText {
        local_name,
        ancestors,
        text,
    });
}

fn xml_local_name(bytes: &[u8]) -> String {
    let name = String::from_utf8_lossy(bytes);
    name.rsplit_once(':')
        .map(|(_, local)| local.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn split_ics_lines(ics: &str) -> Vec<String> {
    ics.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_ics_continuation_line(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

fn ics_property_name(line: &str) -> Option<String> {
    if is_ics_continuation_line(line) {
        return None;
    }
    let (name, _) = line.split_once(':')?;
    Some(
        name.split(';')
            .next()
            .unwrap_or(name)
            .trim()
            .to_ascii_uppercase(),
    )
}

fn normalize_ics_lines(mut lines: Vec<String>) -> String {
    lines.push(String::new());
    lines.join("\r\n")
}

fn start_has_case_insensitive_attribute(
    start: &BytesStart<'_>,
    name: &str,
    expected_value: &str,
) -> bool {
    start
        .attributes()
        .with_checks(false)
        .flatten()
        .any(|attribute| {
            xml_local_name(attribute.key.as_ref()) == name
                && String::from_utf8_lossy(attribute.value.as_ref())
                    .eq_ignore_ascii_case(expected_value)
        })
}

fn decode_xml_general_ref(value: &str) -> String {
    match value {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        value if value.starts_with("#x") || value.starts_with("#X") => {
            decode_numeric_xml_ref(&value[2..], 16).unwrap_or_else(|| format!("&{value};"))
        }
        value if value.starts_with('#') => {
            decode_numeric_xml_ref(&value[1..], 10).unwrap_or_else(|| format!("&{value};"))
        }
        value => format!("&{value};"),
    }
}

fn decode_numeric_xml_ref(value: &str, radix: u32) -> Option<String> {
    let codepoint = u32::from_str_radix(value, radix).ok()?;
    char::from_u32(codepoint).map(|value| value.to_string())
}

fn first_href_for_property(xml: &str, property_name: &str) -> Option<String> {
    let responses = parse_multistatus_responses(xml).ok()?;
    for response in responses {
        let Some(property_index) = response
            .tags
            .iter()
            .position(|tag| tag.local_name == property_name)
        else {
            continue;
        };
        if let Some(href) = response
            .texts
            .iter()
            .find(|entry| entry.local_name == "href" && entry.ancestors.contains(&property_index))
        {
            return Some(href.text.trim().to_string());
        }
    }
    None
}

fn unfold_ics_lines(ics: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw_line in ics.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(raw_line.trim_start());
            }
        } else if !raw_line.is_empty() {
            lines.push(raw_line.to_string());
        }
    }
    lines
}

fn body_with_marker(note: Option<&str>, sync_id: &str) -> String {
    let marker = format!("{MARKER_PREFIX}schedule_block:{sync_id}]");
    match note.map(str::trim).filter(|value| !value.is_empty()) {
        Some(note) => format!("{note}\n\n{marker}"),
        None => marker,
    }
}

fn extract_marker(value: &str) -> Option<String> {
    let start = value.find(MARKER_PREFIX)?;
    let rest = &value[start + MARKER_PREFIX.len()..];
    let end = rest.find(']')?;
    let marker = &rest[..end];
    marker.split_once(':').and_then(|(entity, sync_id)| {
        (entity == ENTITY_SCHEDULE_BLOCK).then(|| sync_id.to_string())
    })
}

fn strip_marker(value: &str) -> String {
    let Some(start) = value.find(MARKER_PREFIX) else {
        return value.trim().to_string();
    };
    let rest = &value[start..];
    let Some(end) = rest.find(']') else {
        return value.trim().to_string();
    };
    format!("{}{}", &value[..start], &rest[end + 1..])
        .trim()
        .to_string()
}

fn escape_ics_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

fn unescape_ics_text(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') | Some('N') => result.push('\n'),
                Some(next) => result.push(next),
                None => result.push(ch),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn local_date_minute_to_ics(date: &str, minute: i64) -> Result<String, String> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|error| error.to_string())?;
    let hour = (minute / 60).clamp(0, 23) as u32;
    let minute = (minute % 60).clamp(0, 59) as u32;
    let datetime = date
        .and_hms_opt(hour, minute, 0)
        .ok_or_else(|| "日历时间无效。".to_string())?;
    Ok(datetime.format("%Y%m%dT%H%M%S").to_string())
}

fn parse_ics_date_time(value: &str) -> Result<(String, i64), String> {
    let datetime = parse_ics_date_time_naive(value)?;
    Ok((
        datetime.date().format("%Y-%m-%d").to_string(),
        datetime.hour() as i64 * 60 + datetime.minute() as i64,
    ))
}

fn parse_ics_date_time_naive(value: &str) -> Result<NaiveDateTime, String> {
    let normalized = value.trim().trim_end_matches('Z');
    if normalized.len() == 8 {
        let date =
            NaiveDate::parse_from_str(normalized, "%Y%m%d").map_err(|error| error.to_string())?;
        return date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| "ICS 日期无效。".to_string());
    }
    NaiveDateTime::parse_from_str(normalized, "%Y%m%dT%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(normalized, "%Y%m%dT%H%M"))
        .map_err(|error| error.to_string())
}

fn parse_ics_date_time_utc(value: &str) -> Result<DateTime<Utc>, String> {
    let naive = parse_ics_date_time_naive(value)?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn parse_rfc3339_millis(value: &str) -> Result<i64, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .map_err(|error| error.to_string())
}

fn millis_to_rfc3339(value: i64) -> String {
    Utc.timestamp_millis_opt(value)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn calendar_sync_range_utc_strings() -> (String, String) {
    let today = Local::now().date_naive();
    let start = today - Duration::days(30);
    let end = today + Duration::days(180);
    (
        start.format("%Y%m%dT000000Z").to_string(),
        end.format("%Y%m%dT235959Z").to_string(),
    )
}

fn date_in_sync_range(date: &str) -> bool {
    let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return false;
    };
    let today = Local::now().date_naive();
    date >= today - Duration::days(30) && date <= today + Duration::days(180)
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

    fn sample_block() -> LocalCalendarBlock {
        LocalCalendarBlock {
            id: 7,
            sync_id: "schedule_block-7".to_string(),
            schedule_date: "2026-06-18".to_string(),
            title: "数学真题".to_string(),
            note: Some("2008 第一套".to_string()),
            start_minute: 8 * 60 + 30,
            end_minute: 10 * 60,
            status: "planned".to_string(),
            updated_at: "2026-06-18T00:00:00Z".to_string(),
            deleted_at: None,
        }
    }

    #[test]
    fn ics_roundtrip_preserves_schedule_block_fields_and_marker() {
        let block = sample_block();
        let ics = build_ics_event(&block, "Asia/Shanghai").expect("build ics");

        assert!(ics.contains("BEGIN:VEVENT"));
        assert!(ics.contains("UID:schedule_block-7@kaoyan-focus"));
        assert!(ics.contains("SUMMARY:数学真题"));
        assert!(ics.contains("X-KAOYAN-FOCUS-SYNC-ID:schedule_block-7"));

        let parsed = parse_ics_event("https://cal.example/cal/7.ics", Some("\"etag-7\""), &ics)
            .expect("parse event")
            .expect("has event");
        assert_eq!(parsed.id, "https://cal.example/cal/7.ics");
        assert_eq!(parsed.title, "数学真题");
        assert_eq!(parsed.note.as_deref(), Some("2008 第一套"));
        assert_eq!(parsed.schedule_date, "2026-06-18");
        assert_eq!(parsed.start_minute, 510);
        assert_eq!(parsed.end_minute, 600);
        assert_eq!(parsed.marker_sync_id.as_deref(), Some("schedule_block-7"));
        assert_eq!(parsed.etag.as_deref(), Some("\"etag-7\""));
    }

    #[test]
    fn marker_writeback_preserves_remote_uid_for_imported_event() {
        let remote_ics = [
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "PRODID:-//Example//Calendar//EN",
            "BEGIN:VEVENT",
            "UID:iphone-event-1",
            "DTSTAMP:20260618T000000Z",
            "DTSTART:20260618T083000",
            "DTEND:20260618T093000",
            "SUMMARY:iPhone 修改的日程",
            "DESCRIPTION:从手机创建",
            "END:VEVENT",
            "END:VCALENDAR",
            "",
        ]
        .join("\r\n");

        let marked =
            mark_remote_ics_with_sync_marker(&remote_ics, "schedule_block-42").expect("mark ics");

        assert!(marked.contains("UID:iphone-event-1"));
        assert!(!marked.contains("UID:schedule_block-42@kaoyan-focus"));
        assert!(marked.contains("X-KAOYAN-FOCUS-SYNC-ID:schedule_block-42"));

        let parsed = parse_ics_event("https://cal.example/cal/plain.ics", None, &marked)
            .expect("parse marked event")
            .expect("has event");
        assert_eq!(parsed.marker_sync_id.as_deref(), Some("schedule_block-42"));
        assert_eq!(parsed.note.as_deref(), Some("从手机创建"));
    }

    #[test]
    fn local_push_to_imported_event_preserves_remote_uid() {
        let remote_ics = [
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "PRODID:-//Example//Calendar//EN",
            "BEGIN:VEVENT",
            "UID:iphone-event-1",
            "DTSTAMP:20260618T000000Z",
            "DTSTART:20260618T083000",
            "DTEND:20260618T093000",
            "SUMMARY:旧标题",
            "DESCRIPTION:旧备注",
            "END:VEVENT",
            "END:VCALENDAR",
            "",
        ]
        .join("\r\n");
        let block = sample_block();

        let updated = build_ics_event_from_existing_remote(&remote_ics, &block, "Asia/Shanghai")
            .expect("build existing event update");

        assert!(updated.contains("UID:iphone-event-1"));
        assert!(!updated.contains("UID:schedule_block-7@kaoyan-focus"));
        assert!(updated.contains("SUMMARY:数学真题"));
        assert!(updated.contains("X-KAOYAN-FOCUS-SYNC-ID:schedule_block-7"));

        let parsed = parse_ics_event("https://cal.example/cal/plain.ics", None, &updated)
            .expect("parse updated event")
            .expect("has event");
        assert_eq!(parsed.marker_sync_id.as_deref(), Some("schedule_block-7"));
        assert_eq!(parsed.title, "数学真题");
        assert_eq!(parsed.note.as_deref(), Some("2008 第一套"));
        assert_eq!(parsed.schedule_date, "2026-06-18");
        assert_eq!(parsed.start_minute, 510);
        assert_eq!(parsed.end_minute, 600);
    }

    #[test]
    fn parses_caldav_calendar_discovery_multistatus() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <d:multistatus xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/" xmlns:cal="urn:ietf:params:xml:ns:caldav">
          <d:response>
            <d:href>/calendars/user/study/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>考研专注</d:displayname>
                <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
                <cal:supported-calendar-component-set>
                  <cal:comp name="VEVENT"/>
                </cal:supported-calendar-component-set>
                <d:current-user-privilege-set>
                  <d:privilege><d:read/></d:privilege>
                  <d:privilege><d:write/></d:privilege>
                </d:current-user-privilege-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/calendars/user/birthdays/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>Birthdays</d:displayname>
                <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
                <cal:supported-calendar-component-set><cal:comp name="VTODO"/></cal:supported-calendar-component-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/calendars/user/readonly/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>Read Only</d:displayname>
                <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
                <cal:supported-calendar-component-set><cal:comp name="VEVENT"/></cal:supported-calendar-component-set>
                <d:current-user-privilege-set>
                  <d:privilege><d:read/></d:privilege>
                </d:current-user-privilege-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let calendars =
            parse_calendar_discovery_response("https://cal.example", xml).expect("parse calendars");
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].name, "考研专注");
        assert_eq!(
            calendars[0].url,
            "https://cal.example/calendars/user/study/"
        );
        assert!(calendars[0].writable);
    }

    #[test]
    fn discovery_accepts_common_caldav_writable_privileges_and_missing_component_set() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
          <d:response>
            <d:href>/calendars/user/icloud-style/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>iCloud Style</d:displayname>
                <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
                <d:current-user-privilege-set>
                  <d:privilege><d:read/></d:privilege>
                  <d:privilege><d:all/></d:privilege>
                </d:current-user-privilege-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/calendars/user/bind-style/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>Bind Style</d:displayname>
                <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
                <cal:supported-calendar-component-set><cal:comp name="VEVENT"/></cal:supported-calendar-component-set>
                <d:current-user-privilege-set>
                  <d:privilege><d:read/></d:privilege>
                  <d:privilege><d:bind/></d:privilege>
                </d:current-user-privilege-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let calendars =
            parse_calendar_discovery_response("https://cal.example", xml).expect("parse calendars");
        assert_eq!(calendars.len(), 2);
        assert_eq!(calendars[0].name, "iCloud Style");
        assert_eq!(calendars[1].name, "Bind Style");
        assert!(calendars.iter().all(|calendar| calendar.writable));
    }

    #[test]
    fn parses_calendar_query_events_with_etags() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
          <d:response>
            <d:href>/calendars/user/study/event-1.ics</d:href>
            <d:propstat>
              <d:prop>
                <d:getetag>"event-1"</d:getetag>
                <cal:calendar-data>BEGIN:VCALENDAR&#13;
VERSION:2.0&#13;
BEGIN:VEVENT&#13;
UID:remote-1&#13;
DTSTAMP:20260618T000000Z&#13;
DTSTART:20260618T083000&#13;
DTEND:20260618T093000&#13;
SUMMARY:英语阅读&#13;
DESCRIPTION:Text 1&#13;
END:VEVENT&#13;
END:VCALENDAR&#13;
</cal:calendar-data>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let events = parse_calendar_query_response("https://cal.example", xml).expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].id,
            "https://cal.example/calendars/user/study/event-1.ics"
        );
        assert_eq!(events[0].etag.as_deref(), Some("\"event-1\""));
        assert_eq!(events[0].title, "英语阅读");
        assert_eq!(events[0].schedule_date, "2026-06-18");
        assert_eq!(events[0].start_minute, 510);
        assert_eq!(events[0].end_minute, 570);
        assert!(events[0].marker_sync_id.is_none());
    }

    #[test]
    fn linked_calendar_action_uses_fingerprints_before_timestamps() {
        assert_eq!(
            linked_calendar_action(1_000, Some(9_000), true, false, false),
            LinkedCalendarAction::PushLocal
        );
        assert_eq!(
            linked_calendar_action(9_000, Some(1_000), false, true, false),
            LinkedCalendarAction::PullRemote
        );
        assert_eq!(
            linked_calendar_action(1_000, Some(9_000), true, true, false),
            LinkedCalendarAction::PullRemote
        );
        assert_eq!(
            linked_calendar_action(5_000, Some(5_000), true, true, true),
            LinkedCalendarAction::PushLocal
        );
    }

    #[test]
    fn imports_plain_remote_event_as_general_schedule_block_and_links_it() {
        let connection = rusqlite::Connection::open_in_memory().expect("db");
        connection
            .execute_batch(
                "
                CREATE TABLE schedule_blocks (
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
                  updated_at TEXT NOT NULL
                );
                CREATE TABLE sync_meta (
                  entity_type TEXT NOT NULL,
                  local_id INTEGER NOT NULL,
                  sync_id TEXT NOT NULL,
                  deleted_at INTEGER,
                  created_at INTEGER NOT NULL DEFAULT 0,
                  updated_at INTEGER NOT NULL,
                  PRIMARY KEY (entity_type, local_id)
                );
                CREATE TABLE calendar_sync_links (
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
                ",
            )
            .expect("schema");

        let remote = RemoteCalendarEvent {
            id: "https://cal.example/cal/plain.ics".to_string(),
            etag: Some("\"plain\"".to_string()),
            title: "政治背诵".to_string(),
            note: Some("第 3 章".to_string()),
            schedule_date: "2026-06-19".to_string(),
            start_minute: 19 * 60,
            end_minute: 20 * 60,
            updated_millis: Some(1_777_000_000_000),
            marker_sync_id: None,
            raw_ics: [
                "BEGIN:VCALENDAR",
                "VERSION:2.0",
                "BEGIN:VEVENT",
                "UID:plain",
                "DTSTART:20260619T190000",
                "DTEND:20260619T200000",
                "SUMMARY:政治背诵",
                "DESCRIPTION:第 3 章",
                "END:VEVENT",
                "END:VCALENDAR",
                "",
            ]
            .join("\r\n"),
        };

        let block =
            create_local_schedule_block_from_remote(&connection, &remote).expect("import remote");
        let imported: (String, String, Option<i64>, String) = connection
            .query_row(
                "
                SELECT b.title, b.category_key, b.subject_id, m.sync_id
                FROM schedule_blocks b
                JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
                ORDER BY b.id DESC
                LIMIT 1
                ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("imported block");
        assert_eq!(imported.0, "政治背诵");
        assert_eq!(imported.1, "general");
        assert_eq!(imported.2, None);
        assert!(imported.3.starts_with("schedule_block-"));
        assert_eq!(block.sync_id, imported.3);

        let link_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM calendar_sync_links", [], |row| {
                row.get(0)
            })
            .expect("link count");
        assert_eq!(link_count, 1);
    }
}

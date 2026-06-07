fn r2_v3_prefix(settings: &ObjectStorageSettings) -> String {
    let key = settings
        .object_key
        .trim()
        .trim_matches('/')
        .replace('\\', "/");
    if key.is_empty() || key == DEFAULT_OBJECT_KEY || key.ends_with(".json") {
        R2_V3_DEFAULT_PREFIX.to_string()
    } else {
        key
    }
}

fn r2_v3_key(settings: &ObjectStorageSettings, child: &str) -> String {
    format!(
        "{}/{}",
        r2_v3_prefix(settings),
        child.trim_start_matches('/')
    )
}

fn r2_v3_manifest_key(settings: &ObjectStorageSettings) -> String {
    r2_v3_key(settings, R2_V3_MANIFEST)
}

fn r2_v3_active_lock_key(settings: &ObjectStorageSettings) -> String {
    r2_v3_key(settings, "runtime/active-lock.json")
}

fn empty_shared_payload(device_id: &str, exported_at: i64) -> SharedSyncPayload {
    SharedSyncPayload {
        schema_version: R2_V3_SCHEMA_VERSION,
        device_id: device_id.to_string(),
        exported_at,
        source_device_id: Some(device_id.to_string()),
        active_device_id: None,
        primary_owner_device_id: None,
        primary_owner_updated_at: None,
        subjects: Vec::new(),
        study_modes: Vec::new(),
        focus_sessions: Vec::new(),
        app_events: Vec::new(),
        checklist_tasks: Vec::new(),
        today_plan_items: Vec::new(),
        schedule_blocks: Vec::new(),
        schedule_templates: Vec::new(),
        daily_reviews: Vec::new(),
        weekly_reviews: Vec::new(),
    }
}

fn sanitize_op_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_logical_suffix(entity_type: &str, sync_id: &str, action: &str) -> i64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!("{entity_type}:{sync_id}:{action}").as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash % (R2_V3_HLC_LOGICAL_MODULUS as u64)) as i64
}

fn operation_seq(updated_at: i64, entity_type: &str, sync_id: &str, action: &str) -> i64 {
    updated_at
        .max(0)
        .saturating_mul(R2_V3_HLC_LOGICAL_MODULUS)
        .saturating_add(stable_logical_suffix(entity_type, sync_id, action))
}

fn hlc_from_updated_at(
    updated_at: i64,
    entity_type: &str,
    sync_id: &str,
    action: &str,
    device_id: &str,
) -> String {
    format!(
        "{:020}-{:05}-{}",
        updated_at.max(0),
        stable_logical_suffix(entity_type, sync_id, action),
        sanitize_op_part(device_id)
    )
}

fn operation_sort_key(operation: &R2V3Operation) -> (i64, String) {
    (operation.seq, operation.op_id.clone())
}

fn op_key_device_seq(key: &str) -> Option<(String, i64)> {
    let mut parts = key.rsplitn(3, '/');
    let file_name = parts.next()?;
    let device_id = parts.next()?.to_string();
    let seq = file_name.split_once('-')?.0.parse::<i64>().ok()?;
    Some((device_id, seq))
}

fn op_id_from_key(key: &str) -> Option<String> {
    key.rsplit('/')
        .next()
        .and_then(|file_name| file_name.split_once('-').map(|(_, op_id)| op_id))
        .map(|op_id| op_id.trim_end_matches(".json").to_string())
}

fn payload_entity_operations(
    payload: &SharedSyncPayload,
    device_id: &str,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Result<Vec<R2V3Operation>, String> {
    let value = serde_json::to_value(payload).map_err(|error| error.to_string())?;
    let fields = [
        ("subjects", "subject"),
        ("studyModes", "study_mode"),
        ("focusSessions", "focus_session"),
        ("appEvents", "app_event"),
        ("checklistTasks", "checklist_task"),
        ("todayPlanItems", "today_plan_item"),
        ("scheduleBlocks", "schedule_block"),
        ("scheduleTemplates", "schedule_template"),
        ("dailyReviews", "daily_review"),
        ("weeklyReviews", "weekly_review"),
    ];

    let mut operations = Vec::new();
    for (field, entity_type) in fields {
        let Some(items) = value.get(field).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let sync_id = item
                .get("syncId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if sync_id.is_empty() {
                continue;
            }
            let updated_at = item
                .get("updatedAt")
                .and_then(Value::as_i64)
                .unwrap_or(payload.exported_at);
            let deleted_at = item.get("deletedAt").and_then(Value::as_i64);
            let action = if deleted_at.is_some() {
                "delete"
            } else {
                "upsert"
            };
            let version_key = (entity_type.to_string(), sync_id.to_string());
            if let Some(version) = entity_versions.get(&version_key) {
                if version.updated_at == updated_at && version.deleted_at == deleted_at {
                    continue;
                }
            }
            let hlc = hlc_from_updated_at(updated_at, entity_type, sync_id, action, device_id);
            let op_id = sanitize_op_part(&format!("{entity_type}-{sync_id}-{hlc}-{action}"));
            operations.push(R2V3Operation {
                schema_version: R2_V3_SCHEMA_VERSION,
                op_id,
                device_id: device_id.to_string(),
                seq: operation_seq(updated_at, entity_type, sync_id, action),
                hlc,
                base_hlc: entity_versions
                    .get(&version_key)
                    .map(|version| version.hlc.clone()),
                entity_type: entity_type.to_string(),
                sync_id: sync_id.to_string(),
                action: action.to_string(),
                payload: Some(item.clone()),
                deleted_at,
            });
        }
    }

    operations.sort_by_key(operation_sort_key);
    operations
        .dedup_by(|left, right| left.op_id == right.op_id && left.device_id == right.device_id);
    Ok(operations)
}

#[derive(Debug, Clone)]
struct ActiveUploadFilterContext {
    is_non_primary: bool,
    active_study_sync_id: Option<String>,
    active_session_sync_id: Option<String>,
    last_accepted_control_at: Option<i64>,
}

fn active_upload_filter_context(
    payload: &SharedSyncPayload,
    device_id: &str,
) -> ActiveUploadFilterContext {
    let owner = payload
        .primary_owner_device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let is_non_primary = owner.is_some_and(|owner| owner != device_id);
    let active_mode = shared_active_study_snapshot(payload).and_then(|snapshot| {
        payload
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == snapshot.sync_id)
    });
    ActiveUploadFilterContext {
        is_non_primary,
        active_study_sync_id: active_mode.map(|mode| mode.sync_id.clone()),
        active_session_sync_id: active_mode.and_then(|mode| mode.current_session_sync_id.clone()),
        last_accepted_control_at: active_mode.and_then(|mode| mode.last_control_at),
    }
}

fn filter_passive_active_upload_operations(
    operations: Vec<R2V3Operation>,
    context: &ActiveUploadFilterContext,
) -> Vec<R2V3Operation> {
    if !context.is_non_primary {
        return operations;
    }
    operations
        .into_iter()
        .filter(|operation| {
            if operation.entity_type == "study_mode"
                && context
                    .active_study_sync_id
                    .as_deref()
                    .is_some_and(|sync_id| sync_id == operation.sync_id)
            {
                return operation_has_local_control_intent(
                    operation,
                    context.last_accepted_control_at,
                );
            }
            if operation.entity_type == "focus_session"
                && context
                    .active_session_sync_id
                    .as_deref()
                    .is_some_and(|sync_id| sync_id == operation.sync_id)
            {
                return false;
            }
            true
        })
        .collect()
}

fn operation_has_local_control_intent(
    operation: &R2V3Operation,
    last_accepted_control_at: Option<i64>,
) -> bool {
    let Some(payload) = operation.payload.as_ref() else {
        return false;
    };
    let Some(control_device_id) = payload
        .get("lastControlDeviceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if control_device_id != operation.device_id {
        return false;
    }
    let Some(action) = payload
        .get("lastControlAction")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| ACTIVE_CONTROL_ACTIONS.contains(value))
    else {
        return false;
    };
    let Some(control_at) = payload.get("lastControlAt").and_then(Value::as_i64) else {
        return false;
    };
    let updated_at = payload
        .get("updatedAt")
        .and_then(Value::as_i64)
        .unwrap_or(control_at);
    control_at > 0
        && updated_at <= control_at.saturating_add(2_000)
        && last_accepted_control_at
            .map(|accepted_at| control_at >= accepted_at && updated_at == control_at)
            .unwrap_or(true)
        && !action.is_empty()
}

fn payload_from_operations(
    operations: &[R2V3Operation],
    device_id: &str,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let mut payload = empty_shared_payload(device_id, exported_at);
    let mut operations = operations.to_vec();
    operations.sort_by_key(operation_sort_key);

    for operation in operations {
        let Some(value) = operation.payload else {
            continue;
        };
        match operation.entity_type.as_str() {
            "subject" => payload.subjects.push(
                serde_json::from_value::<SharedSubject>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "study_mode" => payload.study_modes.push(
                serde_json::from_value::<SharedStudyMode>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "focus_session" => payload.focus_sessions.push(
                serde_json::from_value::<SharedFocusSession>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "app_event" => payload.app_events.push(
                serde_json::from_value::<SharedAppEvent>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "checklist_task" => payload.checklist_tasks.push(
                serde_json::from_value::<SharedChecklistTask>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "today_plan_item" => payload.today_plan_items.push(
                serde_json::from_value::<SharedTodayPlanItem>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "schedule_block" => payload.schedule_blocks.push(
                serde_json::from_value::<SharedScheduleBlock>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "schedule_template" => payload.schedule_templates.push(
                serde_json::from_value::<SharedScheduleTemplate>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "daily_review" => payload.daily_reviews.push(
                serde_json::from_value::<SharedDailyReview>(value)
                    .map_err(|error| error.to_string())?,
            ),
            "weekly_review" => payload.weekly_reviews.push(
                serde_json::from_value::<SharedWeeklyReview>(value)
                    .map_err(|error| error.to_string())?,
            ),
            _ => {}
        }
    }

    Ok(payload)
}

fn apply_operations_to_payload(
    base: SharedSyncPayload,
    operations: &[R2V3Operation],
    device_id: &str,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let ops_payload = payload_from_operations(operations, device_id, exported_at)?;
    Ok(merge_shared_sync_payloads(
        base,
        ops_payload,
        device_id.to_string(),
        exported_at,
    ))
}

fn load_local_entity_versions(
    connection: &Connection,
) -> Result<HashMap<(String, String), LocalEntityVersion>, String> {
    let mut statement = connection
        .prepare(
            "SELECT entity_type, sync_id, hlc, deleted_at, updated_at
             FROM sync_entity_versions",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                LocalEntityVersion {
                    hlc: row.get(2)?,
                    deleted_at: row.get(3)?,
                    updated_at: row.get(4)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut versions = HashMap::new();
    for row in rows {
        let (key, value) = row.map_err(|error| error.to_string())?;
        versions.insert(key, value);
    }
    Ok(versions)
}

fn load_applied_operation_ids(connection: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare("SELECT op_id FROM sync_applied_ops")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut op_ids = HashSet::new();
    for row in rows {
        op_ids.insert(row.map_err(|error| error.to_string())?);
    }
    Ok(op_ids)
}

fn should_apply_incoming_operation(
    current: Option<&LocalEntityVersion>,
    operation: &R2V3Operation,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    if operation.action == "delete" {
        return operation.hlc >= current.hlc;
    }
    if current.deleted_at.is_some() {
        if let Some(base_hlc) = operation.base_hlc.as_deref() {
            if current.hlc.as_str() > base_hlc {
                return false;
            }
        } else if current.hlc >= operation.hlc {
            return false;
        }
    }
    operation.hlc > current.hlc
}

fn operation_version(operation: &R2V3Operation) -> LocalEntityVersion {
    let updated_at = operation
        .payload
        .as_ref()
        .and_then(|payload| payload.get("updatedAt"))
        .and_then(Value::as_i64)
        .or_else(|| {
            operation
                .hlc
                .split('-')
                .next()
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or_default();
    LocalEntityVersion {
        hlc: operation.hlc.clone(),
        deleted_at: operation.deleted_at.or_else(|| {
            operation
                .payload
                .as_ref()
                .and_then(|payload| payload.get("deletedAt"))
                .and_then(Value::as_i64)
        }),
        updated_at,
    }
}

fn filter_incoming_operations(
    operations: Vec<R2V3Operation>,
    applied_operation_ids: &HashSet<String>,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Vec<R2V3Operation> {
    let mut known_versions = entity_versions.clone();
    let mut accepted = Vec::new();
    let mut sorted = operations;
    sorted.sort_by_key(operation_sort_key);

    for operation in sorted {
        if applied_operation_ids.contains(&operation.op_id) {
            continue;
        }
        let key = (operation.entity_type.clone(), operation.sync_id.clone());
        if !should_apply_incoming_operation(known_versions.get(&key), &operation) {
            continue;
        }
        known_versions.insert(key, operation_version(&operation));
        accepted.push(operation);
    }

    accepted
}

fn persist_applied_operations(
    connection: &Connection,
    operations: &[R2V3Operation],
) -> Result<(), String> {
    if operations.is_empty() {
        return Ok(());
    }
    let now = Utc::now().timestamp_millis();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut insert_applied = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_applied_ops (op_id, device_id, seq, hlc, applied_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        let mut upsert_version = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_entity_versions (entity_type, sync_id, hlc, deleted_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        for operation in operations {
            insert_applied
                .execute((
                    &operation.op_id,
                    &operation.device_id,
                    operation.seq,
                    &operation.hlc,
                    now,
                ))
                .map_err(|error| error.to_string())?;
            let version = operation_version(operation);
            upsert_version
                .execute((
                    &operation.entity_type,
                    &operation.sync_id,
                    &version.hlc,
                    version.deleted_at,
                    version.updated_at,
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn persist_payload_entity_versions(
    connection: &Connection,
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<(), String> {
    let current_versions = load_local_entity_versions(connection)?;
    let candidates = payload_entity_version_records(payload, device_id)?;
    let versions: Vec<_> = candidates
        .into_iter()
        .filter(|(entity_type, sync_id, version)| {
            let Some(current) = current_versions.get(&(entity_type.clone(), sync_id.clone()))
            else {
                return true;
            };
            version.hlc > current.hlc
                || (version.hlc == current.hlc
                    && version.updated_at >= current.updated_at
                    && version.deleted_at != current.deleted_at)
        })
        .collect();
    if versions.is_empty() {
        return Ok(());
    }

    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut upsert_version = transaction
            .prepare(
                "INSERT OR REPLACE INTO sync_entity_versions (entity_type, sync_id, hlc, deleted_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| error.to_string())?;
        for (entity_type, sync_id, version) in versions {
            upsert_version
                .execute((
                    entity_type,
                    sync_id,
                    version.hlc,
                    version.deleted_at,
                    version.updated_at,
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn payload_entity_version_records(
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<Vec<(String, String, LocalEntityVersion)>, String> {
    let value = serde_json::to_value(payload).map_err(|error| error.to_string())?;
    let fields = [
        ("subjects", "subject"),
        ("studyModes", "study_mode"),
        ("focusSessions", "focus_session"),
        ("appEvents", "app_event"),
        ("checklistTasks", "checklist_task"),
        ("todayPlanItems", "today_plan_item"),
        ("scheduleBlocks", "schedule_block"),
        ("scheduleTemplates", "schedule_template"),
        ("dailyReviews", "daily_review"),
        ("weeklyReviews", "weekly_review"),
    ];
    let version_device_id = payload
        .source_device_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(device_id);
    let mut versions: HashMap<(String, String), LocalEntityVersion> = HashMap::new();

    for (field, entity_type) in fields {
        let Some(items) = value.get(field).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let sync_id = item
                .get("syncId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if sync_id.is_empty() {
                continue;
            }
            let updated_at = item
                .get("updatedAt")
                .and_then(Value::as_i64)
                .unwrap_or(payload.exported_at);
            let deleted_at = item.get("deletedAt").and_then(Value::as_i64);
            let action = if deleted_at.is_some() {
                "delete"
            } else {
                "upsert"
            };
            let version = LocalEntityVersion {
                hlc: hlc_from_updated_at(
                    updated_at,
                    entity_type,
                    sync_id,
                    action,
                    version_device_id,
                ),
                deleted_at,
                updated_at,
            };
            let key = (entity_type.to_string(), sync_id.to_string());
            let should_replace = versions
                .get(&key)
                .map(|current| version.hlc > current.hlc)
                .unwrap_or(true);
            if should_replace {
                versions.insert(key, version);
            }
        }
    }

    Ok(versions
        .into_iter()
        .map(|((entity_type, sync_id), version)| (entity_type, sync_id, version))
        .collect())
}

fn payload_entity_version_map(
    payload: &SharedSyncPayload,
    device_id: &str,
) -> Result<HashMap<(String, String), LocalEntityVersion>, String> {
    Ok(payload_entity_version_records(payload, device_id)?
        .into_iter()
        .map(|(entity_type, sync_id, version)| ((entity_type, sync_id), version))
        .collect())
}

fn apply_operations_to_version_map(
    mut entity_versions: HashMap<(String, String), LocalEntityVersion>,
    operations: &[R2V3Operation],
) -> HashMap<(String, String), LocalEntityVersion> {
    for operation in operations {
        entity_versions.insert(
            (operation.entity_type.clone(), operation.sync_id.clone()),
            operation_version(operation),
        );
    }
    entity_versions
}

fn with_s3_runtime<T>(
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(R2_OPERATION_TIMEOUT_SECONDS * 3),
            future,
        )
        .await
        .map_err(|_| "R2 operation timed out; sync will retry later".to_string())?
    })
}

async fn object_storage_client(settings: &ObjectStorageSettings) -> Result<S3Client, String> {
    let credentials = Credentials::new(
        settings.access_key_id.clone(),
        settings.secret_access_key.clone(),
        None,
        None,
        "kaoyan-focus-object-storage",
    );
    let timeout_config = TimeoutConfig::builder()
        .connect_timeout(Duration::from_secs(R2_CONNECT_TIMEOUT_SECONDS))
        .read_timeout(Duration::from_secs(R2_READ_TIMEOUT_SECONDS))
        .operation_attempt_timeout(Duration::from_secs(R2_OPERATION_ATTEMPT_TIMEOUT_SECONDS))
        .operation_timeout(Duration::from_secs(R2_OPERATION_TIMEOUT_SECONDS))
        .build();
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .region(Region::new(settings.region.clone()))
        .timeout_config(timeout_config)
        .load()
        .await;
    let config = S3ConfigBuilder::from(&shared_config)
        .endpoint_url(settings.endpoint.clone())
        .force_path_style(true)
        .build();

    Ok(S3Client::from_conf(config))
}

fn is_missing_object_error(message: &str) -> bool {
    message.contains("NotFound") || message.contains("404") || message.contains("NoSuchKey")
}

fn is_precondition_error(message: &str) -> bool {
    message.contains("PreconditionFailed")
        || message.contains("412")
        || message.contains("ConditionalRequestConflict")
        || message.contains("409")
}

fn is_r2_conditional_put_conflict<E, R>(error: &SdkError<E, R>) -> bool
where
    E: ProvideErrorMetadata + std::fmt::Debug,
    R: std::fmt::Debug,
{
    if matches!(
        error.code(),
        Some("PreconditionFailed" | "ConditionalRequestConflict")
    ) {
        return true;
    }

    if let Some(response) = error.raw_response() {
        let text = format!("{response:?}");
        if is_precondition_error(&text) {
            return true;
        }
    }

    is_precondition_error(&format!("{error:?}")) || is_precondition_error(&error.to_string())
}

async fn get_object_bytes_with_etag(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
) -> Result<Option<(Vec<u8>, Option<String>)>, String> {
    match client
        .get_object()
        .bucket(&settings.bucket)
        .key(key)
        .send()
        .await
    {
        Ok(response) => {
            let etag = response.e_tag().map(ToString::to_string);
            let bytes = response
                .body
                .collect()
                .await
                .map_err(|error| format!("Read R2 object body failed: {error}"))?
                .into_bytes();
            Ok(Some((bytes.to_vec(), etag)))
        }
        Err(error) => {
            if error
                .as_service_error()
                .map(|service_error| service_error.is_no_such_key())
                .unwrap_or(false)
            {
                Ok(None)
            } else {
                let message = error.to_string();
                if is_missing_object_error(&message) {
                    Ok(None)
                } else {
                    Err(format!("Download R2 object {key} failed: {error:?}"))
                }
            }
        }
    }
}

async fn delete_object_if_exists(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
) -> Result<(), String> {
    match client
        .delete_object()
        .bucket(&settings.bucket)
        .key(key)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            if is_missing_object_error(&message) {
                Ok(())
            } else {
                Err(format!("Delete R2 object failed: {error}"))
            }
        }
    }
}

async fn put_object_if_none_match(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
    bytes: Vec<u8>,
) -> Result<bool, String> {
    let result = client
        .put_object()
        .bucket(&settings.bucket)
        .key(key)
        .body(ByteStream::from(bytes))
        .content_type("application/json")
        .if_none_match("*")
        .send()
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(error) => {
            if is_r2_conditional_put_conflict(&error) {
                Ok(false)
            } else {
                Err(format!("Upload R2 object failed: {error:?}"))
            }
        }
    }
}

async fn put_manifest_conditionally(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    key: &str,
    bytes: Vec<u8>,
    etag: Option<&str>,
) -> Result<bool, String> {
    let mut request = client
        .put_object()
        .bucket(&settings.bucket)
        .key(key)
        .body(ByteStream::from(bytes))
        .content_type("application/json");

    request = if let Some(etag) = etag {
        request.if_match(etag)
    } else {
        request.if_none_match("*")
    };

    match request.send().await {
        Ok(_) => Ok(true),
        Err(error) => {
            if is_r2_conditional_put_conflict(&error) {
                Ok(false)
            } else {
                Err(format!("Conditional R2 manifest update failed: {error:?}"))
            }
        }
    }
}

async fn try_acquire_r2_v3_active_lock(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
    active_snapshot: &SharedActiveStudySnapshot,
) -> Result<bool, String> {
    let key = r2_v3_active_lock_key(settings);
    let existing = get_object_bytes_with_etag(client, settings, &key).await?;
    let now = Utc::now().timestamp_millis();
    if let Some((bytes, _etag)) = existing.as_ref() {
        if !bytes.is_empty() {
            let lock: R2V3ActiveLock = serde_json::from_slice(bytes)
                .map_err(|error| format!("Parse R2 active lock failed: {error}"))?;
            if lock.expires_at > now
                && !lock.device_id.trim().is_empty()
                && lock.device_id != device_id
            {
                return Ok(false);
            }
        }
    }

    let lock = R2V3ActiveLock {
        schema_version: R2_V3_SCHEMA_VERSION,
        device_id: device_id.to_string(),
        sync_id: active_snapshot.sync_id.clone(),
        state_revision: active_snapshot.state_revision.unwrap_or_default(),
        hlc: hlc_from_updated_at(
            now,
            "active_lock",
            &active_snapshot.sync_id,
            "claim",
            device_id,
        ),
        claimed_at: now,
        expires_at: now + R2_V3_ACTIVE_LOCK_TTL_MILLIS,
    };
    let bytes = serde_json::to_vec(&lock).map_err(|error| error.to_string())?;
    match existing.and_then(|(_, etag)| etag) {
        Some(etag) => put_manifest_conditionally(client, settings, &key, bytes, Some(&etag)).await,
        None => put_object_if_none_match(client, settings, &key, bytes).await,
    }
}

async fn release_r2_v3_active_lock_if_owned(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
) -> Result<(), String> {
    let key = r2_v3_active_lock_key(settings);
    let Some((bytes, etag)) = get_object_bytes_with_etag(client, settings, &key).await? else {
        return Ok(());
    };
    if bytes.is_empty() {
        return Ok(());
    }
    let Ok(existing_lock) = serde_json::from_slice::<R2V3ActiveLock>(&bytes) else {
        return Ok(());
    };
    if existing_lock.device_id != device_id {
        return Ok(());
    }
    let now = Utc::now().timestamp_millis();
    let released_lock = R2V3ActiveLock {
        hlc: hlc_from_updated_at(
            now,
            "active_lock",
            &existing_lock.sync_id,
            "release",
            device_id,
        ),
        claimed_at: now,
        expires_at: now - 1,
        ..existing_lock
    };
    let bytes = serde_json::to_vec(&released_lock).map_err(|error| error.to_string())?;
    if let Some(etag) = etag.as_deref() {
        let _ = put_manifest_conditionally(client, settings, &key, bytes, Some(etag)).await?;
    }
    Ok(())
}

async fn upload_payload_operations(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    operations: &[R2V3Operation],
) -> Result<usize, String> {
    let mut uploaded = 0usize;
    for chunk in operations.chunks(R2_OP_UPLOAD_CONCURRENCY) {
        let mut tasks = Vec::with_capacity(chunk.len());
        for operation in chunk {
            let key = r2_v3_key(
                settings,
                &format!(
                    "ops/{}/{:020}-{}.json",
                    sanitize_op_part(&operation.device_id),
                    operation.seq.max(0),
                    operation.op_id
                ),
            );
            let bytes = serde_json::to_vec(&operation).map_err(|error| error.to_string())?;
            let client = client.clone();
            let settings = settings.clone();
            tasks.push(tokio::spawn(async move {
                put_object_if_none_match(&client, &settings, &key, bytes).await
            }));
        }
        for task in tasks {
            if task
                .await
                .map_err(|error| format!("Upload R2 op task failed: {error}"))??
            {
                uploaded += 1;
            }
        }
    }
    Ok(uploaded)
}

async fn list_r2_v3_operations(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    watermarks: &HashMap<String, i64>,
    applied_operation_ids: &HashSet<String>,
) -> Result<(Vec<R2V3Operation>, HashMap<String, i64>, usize), String> {
    let prefix = r2_v3_key(settings, "ops/");
    let mut continuation: Option<String> = None;
    let mut operations = Vec::new();
    let mut next_watermarks = watermarks.clone();
    let mut bytes_read = 0usize;
    let mut candidate_keys = Vec::new();

    loop {
        let mut request = client
            .list_objects_v2()
            .bucket(&settings.bucket)
            .prefix(&prefix);
        if let Some(token) = continuation.as_deref() {
            request = request.continuation_token(token);
        }
        let output = request
            .send()
            .await
            .map_err(|error| format!("List R2 ops failed: {error}"))?;

        for object in output.contents() {
            let Some(key) = object.key() else {
                continue;
            };
            if !key.ends_with(".json") {
                continue;
            }
            if let Some((device_id, seq)) = op_key_device_seq(key) {
                let watermark = watermarks.get(&device_id).copied().unwrap_or_default();
                if seq <= watermark {
                    continue;
                }
            }
            if op_id_from_key(key)
                .as_ref()
                .is_some_and(|op_id| applied_operation_ids.contains(op_id))
            {
                continue;
            }
            candidate_keys.push(key.to_string());
        }

        continuation = output.next_continuation_token().map(ToString::to_string);
        if continuation.is_none() {
            break;
        }
    }

    for chunk in candidate_keys.chunks(R2_OP_DOWNLOAD_CONCURRENCY) {
        let mut tasks = Vec::with_capacity(chunk.len());
        for key in chunk {
            let client = client.clone();
            let settings = settings.clone();
            let key = key.clone();
            tasks.push(tokio::spawn(async move {
                let bytes = get_object_bytes_with_etag(&client, &settings, &key).await?;
                Ok::<_, String>((key, bytes))
            }));
        }
        for task in tasks {
            let (key, bytes) = task
                .await
                .map_err(|error| format!("Download R2 op task failed: {error}"))??;
            let Some((bytes, _etag)) = bytes else {
                continue;
            };
            bytes_read += bytes.len();
            let operation: R2V3Operation = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Parse R2 op {key} failed: {error}"))?;
            let watermark = watermarks
                .get(&operation.device_id)
                .copied()
                .unwrap_or_default();
            if operation.seq <= watermark {
                continue;
            }
            next_watermarks
                .entry(operation.device_id.clone())
                .and_modify(|value| *value = (*value).max(operation.seq))
                .or_insert(operation.seq);
            operations.push(operation);
        }
    }

    operations.sort_by_key(operation_sort_key);
    Ok((operations, next_watermarks, bytes_read))
}

async fn list_object_storage_backup_objects_for_prefix(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    prefix: &str,
) -> Result<Vec<ObjectStorageBackupObject>, String> {
    let mut continuation: Option<String> = None;
    let mut objects = Vec::new();

    loop {
        let mut request = client
            .list_objects_v2()
            .bucket(&settings.bucket)
            .prefix(prefix);
        if let Some(token) = continuation.as_deref() {
            request = request.continuation_token(token);
        }
        let output = request
            .send()
            .await
            .map_err(|error| format!("List R2/S3 backups failed: {error}"))?;

        for object in output.contents() {
            let Some(key) = object.key() else {
                continue;
            };
            objects.push(ObjectStorageBackupObject {
                key: key.to_string(),
                size: object.size().and_then(|value| u64::try_from(value).ok()),
                last_modified: object.last_modified().and_then(|value| {
                    DateTime::<Utc>::from_timestamp(value.secs(), value.subsec_nanos())
                }),
            });
        }

        continuation = output.next_continuation_token().map(ToString::to_string);
        if continuation.is_none() {
            break;
        }
    }

    Ok(objects)
}

async fn list_all_object_storage_backup_objects(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<Vec<ObjectStorageBackupObject>, String> {
    let mut objects = list_object_storage_backup_objects_for_prefix(
        client,
        settings,
        &r2_v3_key(settings, "backups/"),
    )
    .await?;
    objects.extend(
        list_object_storage_backup_objects_for_prefix(client, settings, R2_LEGACY_BACKUP_PREFIX)
            .await?,
    );
    Ok(objects)
}

async fn load_legacy_v2_payload(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
) -> Result<(SharedSyncPayload, usize, bool), String> {
    let Some((bytes, _etag)) =
        get_object_bytes_with_etag(client, settings, &settings.object_key).await?
    else {
        return Ok((
            empty_shared_payload(device_id, Utc::now().timestamp_millis()),
            0,
            false,
        ));
    };
    if bytes.is_empty() {
        return Ok((
            empty_shared_payload(device_id, Utc::now().timestamp_millis()),
            0,
            false,
        ));
    }
    let payload: SharedSyncPayload = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parse legacy study-sync.json failed: {error}"))?;
    Ok((payload, bytes.len(), true))
}

async fn load_r2_v3_remote_state(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    device_id: &str,
    applied_operation_ids: &HashSet<String>,
    entity_versions: &HashMap<(String, String), LocalEntityVersion>,
) -> Result<R2V3RemoteState, String> {
    let manifest_key = r2_v3_manifest_key(settings);
    let Some((manifest_bytes, manifest_etag)) =
        get_object_bytes_with_etag(client, settings, &manifest_key).await?
    else {
        let (payload, bytes, migrated_legacy) =
            load_legacy_v2_payload(client, settings, device_id).await?;
        return Ok(R2V3RemoteState {
            manifest: None,
            manifest_etag: None,
            payload,
            watermarks: HashMap::new(),
            operation_count: 0,
            bytes,
            migrated_legacy,
            applied_operations: Vec::new(),
        });
    };

    let manifest: R2V3Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Parse R2 manifest failed: {error}"))?;
    let exported_at = Utc::now().timestamp_millis();
    let snapshot = if let Some(snapshot_key) = manifest.current_snapshot_key.as_deref() {
        match get_object_bytes_with_etag(client, settings, snapshot_key).await? {
            Some((snapshot_bytes, _etag)) => Some(
                serde_json::from_slice::<R2V3Snapshot>(&snapshot_bytes)
                    .map_err(|error| format!("Parse R2 snapshot failed: {error}"))?,
            ),
            None => None,
        }
    } else {
        None
    };
    let mut payload = snapshot
        .as_ref()
        .map(|snapshot| snapshot.payload.clone())
        .unwrap_or_else(|| empty_shared_payload(device_id, exported_at));
    if manifest.primary_owner_device_id.is_some() || manifest.primary_owner_updated_at.is_some() {
        payload.primary_owner_device_id = manifest.primary_owner_device_id.clone();
        payload.primary_owner_updated_at = manifest.primary_owner_updated_at;
    }
    let snapshot_watermarks = snapshot
        .as_ref()
        .map(|snapshot| snapshot.watermarks.clone())
        .unwrap_or_default();

    let (operations, watermarks, op_bytes) = list_r2_v3_operations(
        client,
        settings,
        &snapshot_watermarks,
        applied_operation_ids,
    )
    .await?;
    let applied_operations =
        filter_incoming_operations(operations, applied_operation_ids, entity_versions);
    if !applied_operations.is_empty() {
        payload =
            apply_operations_to_payload(payload, &applied_operations, device_id, exported_at)?;
    }

    Ok(R2V3RemoteState {
        manifest: Some(manifest),
        manifest_etag,
        payload,
        watermarks,
        operation_count: applied_operations.len(),
        bytes: manifest_bytes.len() + op_bytes,
        migrated_legacy: false,
        applied_operations,
    })
}

async fn write_r2_v3_snapshot_and_manifest(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    mut payload: SharedSyncPayload,
    watermarks: HashMap<String, i64>,
    manifest_etag: Option<&str>,
    device_id: &str,
) -> Result<bool, String> {
    let now = Utc::now();
    payload.schema_version = R2_V3_SCHEMA_VERSION;
    let snapshot_id = format!("{}-{}", now.format("%Y%m%dT%H%M%S%.3fZ"), Uuid::new_v4());
    let snapshot_key = r2_v3_key(settings, &format!("snapshots/{snapshot_id}.json"));
    let backup_key = r2_v3_key(
        settings,
        &format!(
            "backups/{}-{}.json",
            now.format("%Y%m%dT%H%M%S%.3fZ"),
            sanitize_op_part(device_id)
        ),
    );
    let snapshot = R2V3Snapshot {
        schema_version: R2_V3_SCHEMA_VERSION,
        snapshot_id: snapshot_id.clone(),
        created_hlc: hlc_from_updated_at(
            now.timestamp_millis(),
            "snapshot",
            &snapshot_id,
            "compact",
            device_id,
        ),
        watermarks: watermarks.clone(),
        payload,
    };
    let snapshot_bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    let _ = put_object_if_none_match(client, settings, &backup_key, snapshot_bytes.clone()).await?;
    prune_object_storage_backups_with_client_best_effort(client, settings).await;
    put_object_if_none_match(client, settings, &snapshot_key, snapshot_bytes).await?;

    let manifest = R2V3Manifest {
        schema_version: R2_V3_SCHEMA_VERSION,
        current_snapshot_key: Some(snapshot_key),
        watermarks,
        primary_owner_device_id: snapshot.payload.primary_owner_device_id.clone(),
        primary_owner_updated_at: snapshot.payload.primary_owner_updated_at,
        compacted_at: now.to_rfc3339(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    put_manifest_conditionally(
        client,
        settings,
        &r2_v3_manifest_key(settings),
        manifest_bytes,
        manifest_etag,
    )
    .await
}

async fn write_r2_v3_manifest(
    client: &S3Client,
    settings: &ObjectStorageSettings,
    current_snapshot_key: Option<String>,
    watermarks: HashMap<String, i64>,
    primary_owner_device_id: Option<String>,
    primary_owner_updated_at: Option<i64>,
    manifest_etag: Option<&str>,
) -> Result<bool, String> {
    let manifest = R2V3Manifest {
        schema_version: R2_V3_SCHEMA_VERSION,
        current_snapshot_key,
        watermarks,
        primary_owner_device_id,
        primary_owner_updated_at,
        compacted_at: Utc::now().to_rfc3339(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    put_manifest_conditionally(
        client,
        settings,
        &r2_v3_manifest_key(settings),
        manifest_bytes,
        manifest_etag,
    )
    .await
}

async fn fetch_object_storage_metadata(
    client: &S3Client,
    settings: &ObjectStorageSettings,
) -> Result<RemoteFileMetadata, String> {
    match client
        .head_object()
        .bucket(&settings.bucket)
        .key(&settings.object_key)
        .send()
        .await
    {
        Ok(output) => Ok(RemoteFileMetadata {
            exists: true,
            size: output
                .content_length()
                .and_then(|value| u64::try_from(value).ok()),
            last_modified: output.last_modified().and_then(|value| {
                DateTime::<Utc>::from_timestamp(value.secs(), value.subsec_nanos())
            }),
        }),
        Err(error) => {
            let message = error.to_string();
            if message.contains("NotFound")
                || message.contains("404")
                || message.contains("NoSuchKey")
            {
                Ok(RemoteFileMetadata {

                    exists: false,
                    size: None,
                    last_modified: None,
                })
            } else {
                Err(format!("读取对象存储远程文件元数据失败：{error}"))
            }
        }
    }
}


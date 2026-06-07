pub fn export_shared_sync_payload(
    connection: &Connection,
    device_id: String,
    exported_at: i64,
) -> Result<SharedSyncPayload, String> {
    let mut payload = SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION,
        device_id: device_id.clone(),
        exported_at,
        source_device_id: Some(device_id.clone()),
        active_device_id: active_device_id(connection).ok().flatten(),
        primary_owner_device_id: primary_owner_device_id(connection).ok().flatten(),
        primary_owner_updated_at: primary_owner_updated_at(connection).ok().flatten(),
        subjects: export_subjects(connection)?,
        study_modes: export_study_modes(connection)?,
        focus_sessions: export_focus_sessions(connection)?,
        app_events: export_app_events(connection)?,
        checklist_tasks: export_checklist_tasks(connection)?,
        today_plan_items: export_today_plan_items(connection)?,
        schedule_blocks: export_schedule_blocks(connection)?,
        schedule_templates: export_schedule_templates(connection)?,
        daily_reviews: export_daily_reviews(connection)?,
        weekly_reviews: export_weekly_reviews(connection)?,
    };
    canonicalize_subject_payload(&mut payload);
    Ok(payload)
}

pub fn merge_shared_sync_payloads(
    mut local: SharedSyncPayload,
    mut remote: SharedSyncPayload,
    device_id: String,
    exported_at: i64,
) -> SharedSyncPayload {
    canonicalize_subject_payload(&mut local);
    canonicalize_subject_payload(&mut remote);
    let local_active = shared_active_study_snapshot(&local);
    let remote_active = shared_active_study_snapshot(&remote);
    let (primary_owner_device_id, primary_owner_updated_at) = merge_primary_owner(&local, &remote);
    let active_merge_decision = classify_active_remote_merge(
        &local,
        &remote,
        local_active.as_ref(),
        exported_at,
        primary_owner_device_id.as_deref(),
    );
    let remote_primary_active = primary_owner_device_id.as_deref().is_some_and(|owner| {
        remote.device_id == owner && local.device_id != owner && remote_active.is_some()
    });
    let keep_local_active = !remote_primary_active
        && (primary_owner_prefers_local(
            primary_owner_device_id.as_deref(),
            &local,
            &remote,
            local_active.as_ref(),
            remote_active.as_ref(),
        ) || should_keep_local_active(
            &local,
            &remote,
            local_active.as_ref(),
            remote_active.as_ref(),
            exported_at,
        ) && active_merge_decision != ActiveRemoteMergeDecision::AcceptRemoteCommand
            || active_merge_decision == ActiveRemoteMergeDecision::KeepLocalActive);
    let preferred_active_sync_id = match (&local_active, &remote_active) {
        (Some(local_snapshot), Some(_)) if keep_local_active => {
            Some(local_snapshot.sync_id.clone())
        }
        (_, Some(remote_snapshot)) if remote_primary_active => {
            Some(remote_snapshot.sync_id.clone())
        }
        _ => None,
    };
    let mut study_modes = merge_study_modes(&local.study_modes, &remote.study_modes);
    let mut focus_sessions = merge_focus_sessions(&local.focus_sessions, &remote.focus_sessions);
    resolve_shared_active_conflicts(
        &mut study_modes,
        &mut focus_sessions,
        exported_at,
        preferred_active_sync_id.as_deref(),
    );
    if keep_local_active {
        if let Some(snapshot) = local_active.as_ref() {
            restore_active_from_payload(&mut study_modes, &mut focus_sessions, &local, snapshot);
        }
    } else if remote_primary_active {
        if let Some(snapshot) = remote_active.as_ref() {
            restore_active_from_payload(&mut study_modes, &mut focus_sessions, &remote, snapshot);
        }
    } else if active_merge_decision == ActiveRemoteMergeDecision::AcceptRemoteCommand {
        restore_matching_active_from_remote(
            &mut study_modes,
            &mut focus_sessions,
            &remote,
            &local,
            local_active.as_ref(),
            exported_at,
        );
    }

    SharedSyncPayload {
        schema_version: SYNC_SCHEMA_VERSION
            .max(local.schema_version)
            .max(remote.schema_version),
        device_id: device_id.clone(),
        exported_at,
        source_device_id: Some(device_id.clone()),
        active_device_id: shared_active_study_snapshot_from_modes(&study_modes).and_then(
            |snapshot| {
                if local.active_device_id.is_some()
                    && local.device_id == device_id
                    && local
                        .study_modes
                        .iter()
                        .any(|mode| mode.sync_id == snapshot.sync_id)
                {
                    local.active_device_id.clone()
                } else if remote.active_device_id.is_some()
                    && remote
                        .study_modes
                        .iter()
                        .any(|mode| mode.sync_id == snapshot.sync_id)
                {
                    remote.active_device_id.clone()
                } else {
                    Some(device_id.clone())
                }
            },
        ),
        primary_owner_device_id: primary_owner_device_id.clone(),
        primary_owner_updated_at,
        subjects: merge_latest_by_sync_id(
            &local.subjects,
            &remote.subjects,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        study_modes,
        focus_sessions,
        app_events: merge_latest_by_sync_id(
            &local.app_events,
            &remote.app_events,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        checklist_tasks: merge_latest_by_sync_id(
            &local.checklist_tasks,
            &remote.checklist_tasks,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        today_plan_items: merge_latest_by_sync_id(
            &local.today_plan_items,
            &remote.today_plan_items,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        schedule_blocks: merge_latest_by_sync_id(
            &local.schedule_blocks,
            &remote.schedule_blocks,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        schedule_templates: merge_latest_by_sync_id(
            &local.schedule_templates,
            &remote.schedule_templates,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        daily_reviews: merge_latest_by_sync_id(
            &local.daily_reviews,
            &remote.daily_reviews,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
        weekly_reviews: merge_latest_by_sync_id(
            &local.weekly_reviews,
            &remote.weekly_reviews,
            |item| item.sync_id.as_str(),
            |item| item.updated_at,
            |item| item.deleted_at,
        ),
    }
}

pub fn merge_remote_payload_into_local(
    local: SharedSyncPayload,
    remote: SharedSyncPayload,
    device_id: String,
    exported_at: i64,
) -> SharedSyncPayload {
    let mut local = local;
    let mut remote = remote;
    canonicalize_subject_payload(&mut local);
    canonicalize_subject_payload(&mut remote);
    merge_shared_sync_payloads(local, remote, device_id, exported_at)
}

pub fn count_payload_entities(payload: &SharedSyncPayload) -> i64 {
    [
        payload.subjects.len(),
        payload.study_modes.len(),
        payload.focus_sessions.len(),
        payload.app_events.len(),
        payload.checklist_tasks.len(),
        payload.today_plan_items.len(),
        payload.schedule_blocks.len(),
        payload.schedule_templates.len(),
        payload.daily_reviews.len(),
        payload.weekly_reviews.len(),
    ]
    .iter()
    .map(|value| *value as i64)
    .sum()
}

pub fn count_payload_deleted_entities(payload: &SharedSyncPayload) -> i64 {
    payload
        .subjects
        .iter()
        .filter(|item| item.deleted_at.is_some())
        .count() as i64
        + payload
            .study_modes
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .focus_sessions
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .app_events
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .checklist_tasks
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .today_plan_items
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .schedule_blocks
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .schedule_templates
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .daily_reviews
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
        + payload
            .weekly_reviews
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .count() as i64
}

fn active_device_id(connection: &Connection) -> Result<Option<String>, String> {
    let active_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM study_modes WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_exists > 0 {
        ensure_device_id(connection).map(Some)
    } else {
        Ok(None)
    }
}

fn primary_owner_device_id(connection: &Connection) -> Result<Option<String>, String> {
    let value = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'primary_owner_device_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(value.and_then(|item| normalize_optional_device_id(&item)))
}

fn primary_owner_updated_at(connection: &Connection) -> Result<Option<i64>, String> {
    let value = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'primary_owner_updated_at'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(value
        .and_then(|item| item.trim().parse::<i64>().ok())
        .filter(|value| *value > 0))
}

fn set_primary_owner(
    connection: &Connection,
    owner: Option<&str>,
    updated_at: Option<i64>,
) -> Result<(), String> {
    let value = owner
        .and_then(normalize_optional_device_id)
        .unwrap_or_default();
    let updated_at_value = updated_at.unwrap_or(0).max(0).to_string();
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES ('primary_owner_device_id', ?1, ?2)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![value, now],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES ('primary_owner_updated_at', ?1, ?2)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![updated_at_value, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn normalize_optional_device_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn shared_active_study_snapshot(
    payload: &SharedSyncPayload,
) -> Option<SharedActiveStudySnapshot> {
    shared_active_study_snapshot_from_modes(&payload.study_modes)
}

fn shared_active_study_snapshot_from_modes(
    study_modes: &[SharedStudyMode],
) -> Option<SharedActiveStudySnapshot> {
    study_modes
        .iter()
        .filter(|item| item.deleted_at.is_none())
        .filter(|item| {
            item.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .fold(
            None,
            |best: Option<(&SharedStudyMode, (i64, i64))>, item| {
                let score = active_sort_key(item);
                match best {
                    Some((current, current_score)) if current_score >= score => {
                        Some((current, current_score))
                    }
                    _ => Some((item, score)),
                }
            },
        )
        .map(|(item, _)| SharedActiveStudySnapshot {
            sync_id: item.sync_id.clone(),
            status: item.status.clone(),
            phase: item.phase.clone(),
            subject_sync_id: item.subject_sync_id.clone(),
            current_session_sync_id: item.current_session_sync_id.clone(),
            phase_started_at: item.phase_started_at,
            paused_at: item.paused_at,
            round_number: item.round_number,
            current_break_type: item.current_break_type.clone(),
            ended_at: item.ended_at,
            state_revision: item.state_revision,
            updated_at: item.updated_at,
        })
}

fn canonicalize_subject_payload(payload: &mut SharedSyncPayload) {
    let mut aliases = HashMap::<String, String>::new();
    for legacy in [
        ("subject-politics", "subject-1"),
        ("subject-english", "subject-2"),
        ("subject-math", "subject-3"),
        ("subject-major", "subject-4"),
    ] {
        aliases.insert(legacy.0.to_string(), legacy.1.to_string());
    }

    for subject in &payload.subjects {
        let canonical =
            canonical_subject_sync_id_for_subject(&subject.sync_id, subject.name.as_deref());
        if canonical != subject.sync_id {
            aliases.insert(subject.sync_id.clone(), canonical);
        }
    }

    for subject in &mut payload.subjects {
        subject.sync_id = aliases
            .get(&subject.sync_id)
            .cloned()
            .unwrap_or_else(|| canonical_subject_sync_id(&subject.sync_id));
    }
    payload.subjects.retain(|subject| {
        !(is_default_subject_sync_id(&subject.sync_id) && subject.deleted_at.is_some())
    });
    for mode in &mut payload.study_modes {
        canonicalize_subject_option(&mut mode.subject_sync_id, &aliases);
    }
    for session in &mut payload.focus_sessions {
        canonicalize_subject_option(&mut session.subject_sync_id, &aliases);
    }
    for task in &mut payload.checklist_tasks {
        canonicalize_subject_option(&mut task.subject_sync_id, &aliases);
    }
    for item in &mut payload.today_plan_items {
        canonicalize_subject_option(&mut item.subject_sync_id, &aliases);
    }
    for block in &mut payload.schedule_blocks {
        canonicalize_subject_option(&mut block.subject_sync_id, &aliases);
    }
    for template in &mut payload.schedule_templates {
        canonicalize_subject_option(&mut template.subject_sync_id, &aliases);
    }
}

fn normalize_import_active_sessions(payload: &mut SharedSyncPayload) {
    let active_mode_ids = payload
        .study_modes
        .iter()
        .filter(|mode| mode.deleted_at.is_none())
        .filter(|mode| {
            mode.status
                .as_deref()
                .map(is_shared_running_status)
                .unwrap_or(false)
        })
        .map(|mode| mode.sync_id.clone())
        .collect::<HashSet<_>>();
    let active_session_ids = payload
        .study_modes
        .iter()
        .filter_map(|mode| mode.current_session_sync_id.as_deref())
        .map(str::to_string)
        .collect::<HashSet<_>>();

    for session in &mut payload.focus_sessions {
        if session.deleted_at.is_some() || session.status.as_deref() != Some("running") {
            continue;
        }
        let belongs_to_active_mode = session
            .study_mode_sync_id
            .as_deref()
            .map(|mode_id| active_mode_ids.contains(mode_id))
            .unwrap_or(false);
        if active_session_ids.contains(&session.sync_id) || belongs_to_active_mode {
            continue;
        }

        session.status = Some("finished".to_string());
        session.ended_at = session.ended_at.or(Some(session.updated_at));
        session.end_reason = session
            .end_reason
            .clone()
            .or_else(|| Some("sync_takeover".to_string()));
    }
}

fn canonicalize_subject_option(value: &mut Option<String>, aliases: &HashMap<String, String>) {
    if let Some(sync_id) = value.as_deref() {
        *value = Some(
            aliases
                .get(sync_id)
                .cloned()
                .unwrap_or_else(|| canonical_subject_sync_id(sync_id)),
        );
    }
}

fn canonical_subject_sync_id_for_subject(sync_id: &str, name: Option<&str>) -> String {
    if let Some(canonical) = name.and_then(canonical_subject_sync_id_for_name) {
        return canonical.to_string();
    }
    canonical_subject_sync_id(sync_id)
}

fn canonical_subject_sync_id(sync_id: &str) -> String {
    match sync_id.trim() {
        "subject-politics" => "subject-1".to_string(),
        "subject-english" => "subject-2".to_string(),
        "subject-math" => "subject-3".to_string(),
        "subject-major" => "subject-4".to_string(),
        value => value.to_string(),
    }
}

fn canonical_subject_sync_id_for_name(name: &str) -> Option<&'static str> {
    match normalize_name(name).as_str() {
        "政治" => Some(DEFAULT_SUBJECT_SYNC_IDS[0]),
        "英语" => Some(DEFAULT_SUBJECT_SYNC_IDS[1]),
        "数学" => Some(DEFAULT_SUBJECT_SYNC_IDS[2]),
        "专业课" => Some(DEFAULT_SUBJECT_SYNC_IDS[3]),
        _ => None,
    }
}

fn is_default_subject_sync_id(sync_id: &str) -> bool {
    DEFAULT_SUBJECT_SYNC_IDS.contains(&sync_id)
}


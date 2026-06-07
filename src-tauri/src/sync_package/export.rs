fn export_subjects(connection: &Connection) -> Result<Vec<SharedSubject>, String> {
    let rows = load_subject_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        if !row.enabled {
            continue;
        }
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let preferred_sync_id = default_subject_sync_id(&row.name, row.id);
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_SUBJECT,
            row.id,
            Some(preferred_sync_id),
            updated_at,
        )?;
        let meta = get_sync_meta_by_sync_id(connection, &sync_id)?;
        if meta.as_ref().and_then(|item| item.deleted_at).is_some() {
            continue;
        }

        payload.push(SharedSubject {
            sync_id,
            name: Some(row.name),
            color: row.color,
            enabled: Some(row.enabled),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SUBJECT)?);
    Ok(payload)
}

fn export_study_modes(connection: &Connection) -> Result<Vec<SharedStudyMode>, String> {
    let rows = load_study_mode_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let started_at_millis = parse_rfc3339_millis(&row.started_at)?;
        let phase_started_at_millis = parse_rfc3339_millis(&row.phase_started_at)?;
        let paused_at_millis = row
            .paused_at
            .as_deref()
            .map(parse_rfc3339_millis)
            .transpose()?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_STUDY_MODE, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedStudyMode {
            sync_id,
            state_revision: Some(row.state_revision.max(1)),
            mode: Some(row.mode),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            planned_seconds: Some(row.planned_seconds),
            focus_seconds: Some(row.focus_seconds),
            break_seconds: Some(row.break_seconds),
            long_break_seconds: Some(row.long_break_seconds),
            long_break_interval: Some(row.long_break_interval),
            phase: Some(if row.paused_at.is_some() {
                "paused".to_string()
            } else {
                row.phase.clone()
            }),
            round_number: Some(row.cycle_index),
            started_at: Some(started_at_millis),
            phase_started_at: Some(phase_started_at_millis),
            paused_at: paused_at_millis,
            paused_from_phase: row.paused_at.as_ref().map(|_| row.phase.clone()),
            accumulated_study_seconds: Some(row.accumulated_study_seconds),
            total_paused_seconds: Some(row.total_paused_seconds),
            phase_paused_seconds: Some(row.paused_stage_elapsed_seconds),
            paused_stage_elapsed_seconds: Some(row.paused_stage_elapsed_seconds),
            current_break_type: Some(break_type_after_round(
                row.cycle_index,
                row.long_break_interval,
            )),
            ended_at: row
                .ended_at
                .as_deref()
                .map(parse_rfc3339_millis)
                .transpose()?,
            current_session_sync_id: row.current_session_id.and_then(|session_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_FOCUS_SESSION,
                    Some(session_id),
                )
                .ok()
                .flatten()
            }),
            schedule_block_sync_id: row.schedule_block_id.and_then(|block_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_BLOCK,
                    Some(block_id),
                )
                .ok()
                .flatten()
            }),
            today_plan_item_sync_id: row.today_plan_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            last_control_device_id: row.last_control_device_id,
            last_control_action: row.last_control_action,
            last_control_at: row.last_control_at,
            status: Some(to_shared_study_status(&row.status).to_string()),
            finish_reason: row.finish_reason,
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_STUDY_MODE)?);
    Ok(payload)
}

fn export_focus_sessions(connection: &Connection) -> Result<Vec<SharedFocusSession>, String> {
    let rows = load_focus_session_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_FOCUS_SESSION, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedFocusSession {
            sync_id,
            study_mode_sync_id: resolve_study_mode_sync_id_for_session(connection, row.id)?,
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            mode: Some(row.mode),
            planned_seconds: Some(row.planned_seconds),
            actual_seconds: Some(row.actual_seconds),
            started_at: Some(parse_rfc3339_millis(&row.started_at)?),
            ended_at: row
                .ended_at
                .as_deref()
                .map(parse_rfc3339_millis)
                .transpose()?,
            status: Some(row.status),
            end_reason: row.end_reason,
            interruption_count: Some(row.interruption_count),
            emergency_exit_count: Some(row.emergency_exit_count),
            paused_seconds: Some(row.paused_seconds),
            followed_by_break_type: row.followed_by_break_type,
            schedule_block_sync_id: row.schedule_block_id.and_then(|block_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_BLOCK,
                    Some(block_id),
                )
                .ok()
                .flatten()
            }),
            today_plan_item_sync_id: row.today_plan_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_FOCUS_SESSION)?);
    Ok(payload)
}

fn export_app_events(connection: &Connection) -> Result<Vec<SharedAppEvent>, String> {
    let rows = load_app_event_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let created_at = parse_rfc3339_millis(&row.created_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_APP_EVENT, row.id, None, created_at)?;
        payload.push(SharedAppEvent {
            sync_id,
            study_mode_sync_id: None,
            focus_session_sync_id: resolve_existing_sync_id_by_local_id(
                connection,
                ENTITY_FOCUS_SESSION,
                Some(row.session_id),
            )?,
            package_name: Some(row.process_name),
            app_name: row.window_title.clone(),
            event_type: Some(row.event_type),
            action: row.action_taken.clone(),
            created_at: Some(created_at),
            updated_at: created_at,
            deleted_at: None,
        });
    }
    Ok(payload)
}

fn export_checklist_tasks(connection: &Connection) -> Result<Vec<SharedChecklistTask>, String> {
    let rows = load_checklist_task_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_CHECKLIST_TASK, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedChecklistTask {
            sync_id,
            category_key: Some(map_board_scope_to_category_key(&row.board_scope)),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            title: Some(row.title),
            note: row.note,
            due_date: row.due_date,
            sort_order: Some(row.sort_order as f64),
            completed: Some(row.completed),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_CHECKLIST_TASK)?);
    Ok(payload)
}

fn export_today_plan_items(connection: &Connection) -> Result<Vec<SharedTodayPlanItem>, String> {
    let rows = load_today_plan_item_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            row.id,
            None,
            updated_at,
        )?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedTodayPlanItem {
            sync_id,
            today_date: Some(row.today_date),
            source_task_sync_id: row.source_task_id.and_then(|source_task_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_CHECKLIST_TASK,
                    Some(source_task_id),
                )
                .ok()
                .flatten()
            }),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            title: Some(row.title),
            note: row.note,
            due_date: row.due_date,
            sort_order: Some(row.sort_order as f64),
            completed: Some(row.completed),
            synced_source_completion: Some(row.synced_source_completion),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_TODAY_PLAN_ITEM)?);
    Ok(payload)
}

fn export_schedule_blocks(connection: &Connection) -> Result<Vec<SharedScheduleBlock>, String> {
    let rows = load_schedule_block_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_SCHEDULE_BLOCK, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedScheduleBlock {
            sync_id,
            schedule_date: Some(row.schedule_date),
            title: Some(row.title),
            note: row.note,
            category_key: Some(row.category_key),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            source_today_item_sync_id: row.source_today_item_id.and_then(|today_item_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_TODAY_PLAN_ITEM,
                    Some(today_item_id),
                )
                .ok()
                .flatten()
            }),
            template_sync_id: row.template_id.and_then(|template_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_SCHEDULE_TEMPLATE,
                    Some(template_id),
                )
                .ok()
                .flatten()
            }),
            start_minute: Some(row.start_minute),
            end_minute: Some(row.end_minute),
            status: Some(row.status),
            linked_study_mode_sync_id: row.linked_study_mode_id.and_then(|study_mode_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_STUDY_MODE,
                    Some(study_mode_id),
                )
                .ok()
                .flatten()
            }),
            linked_focus_session_sync_id: row.linked_focus_session_id.and_then(|session_id| {
                resolve_existing_sync_id_by_local_id(
                    connection,
                    ENTITY_FOCUS_SESSION,
                    Some(session_id),
                )
                .ok()
                .flatten()
            }),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SCHEDULE_BLOCK)?);
    Ok(payload)
}

fn export_schedule_templates(
    connection: &Connection,
) -> Result<Vec<SharedScheduleTemplate>, String> {
    let rows = load_schedule_template_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id = resolve_or_create_sync_id(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            row.id,
            None,
            updated_at,
        )?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedScheduleTemplate {
            sync_id,
            title: Some(row.title),
            note: row.note,
            category_key: Some(row.category_key),
            subject_sync_id: row.subject_id.and_then(|subject_id| {
                resolve_existing_sync_id_by_local_id(connection, ENTITY_SUBJECT, Some(subject_id))
                    .ok()
                    .flatten()
            }),
            weekdays: Some(parse_weekdays_json(&row.weekdays)),
            start_minute: Some(row.start_minute),
            end_minute: Some(row.end_minute),
            enabled: Some(row.enabled),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_SCHEDULE_TEMPLATE)?);
    Ok(payload)
}

fn export_daily_reviews(connection: &Connection) -> Result<Vec<SharedDailyReview>, String> {
    let rows = load_daily_review_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_DAILY_REVIEW, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedDailyReview {
            sync_id,
            review_date: Some(row.review_date),
            summary: row.summary,
            blockers: row.blockers,
            tomorrow_focus: row.tomorrow_focus,
            mood_score: Some(row.mood_score),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_DAILY_REVIEW)?);
    Ok(payload)
}

fn export_weekly_reviews(connection: &Connection) -> Result<Vec<SharedWeeklyReview>, String> {
    let rows = load_weekly_review_rows(connection)?;
    let mut payload = Vec::new();
    for row in rows {
        let updated_at = parse_rfc3339_millis(&row.updated_at)?;
        let sync_id =
            resolve_or_create_sync_id(connection, ENTITY_WEEKLY_REVIEW, row.id, None, updated_at)?;
        if get_sync_meta_by_sync_id(connection, &sync_id)?
            .and_then(|item| item.deleted_at)
            .is_some()
        {
            continue;
        }

        payload.push(SharedWeeklyReview {
            sync_id,
            week_start_date: Some(row.week_start_date),
            summary: row.summary,
            blockers: row.blockers,
            next_week_focus: row.next_week_focus,
            mood_score: Some(row.mood_score),
            created_at: Some(parse_rfc3339_millis(&row.created_at)?),
            updated_at,
            deleted_at: None,
        });
    }

    payload.extend(export_tombstones(connection, ENTITY_WEEKLY_REVIEW)?);
    Ok(payload)
}

fn export_tombstones<T>(connection: &Connection, entity_type: &str) -> Result<Vec<T>, String>
where
    T: From<DeletedPayload>,
{
    let mut statement = connection
        .prepare(
            "
            SELECT sync_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND deleted_at IS NOT NULL
            ORDER BY deleted_at ASC, local_id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![entity_type], |row| {
            Ok(DeletedPayload {
                sync_id: row.get(0)?,
                deleted_at: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|item| item.map(Into::into).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
}

#[derive(Debug, Clone)]
struct DeletedPayload {
    sync_id: String,
    deleted_at: Option<i64>,
}

impl From<DeletedPayload> for SharedSubject {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            name: None,
            color: None,
            enabled: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedStudyMode {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            state_revision: None,
            mode: None,
            subject_sync_id: None,
            planned_seconds: None,
            focus_seconds: None,
            break_seconds: None,
            long_break_seconds: None,
            long_break_interval: None,
            phase: None,
            round_number: None,
            started_at: None,
            phase_started_at: None,
            paused_at: None,
            paused_from_phase: None,
            accumulated_study_seconds: None,
            total_paused_seconds: None,
            phase_paused_seconds: None,
            paused_stage_elapsed_seconds: None,
            current_break_type: None,
            ended_at: None,
            current_session_sync_id: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            last_control_device_id: None,
            last_control_action: None,
            last_control_at: None,
            status: None,
            finish_reason: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedFocusSession {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            study_mode_sync_id: None,
            subject_sync_id: None,
            mode: None,
            planned_seconds: None,
            actual_seconds: None,
            started_at: None,
            ended_at: None,
            status: None,
            end_reason: None,
            interruption_count: None,
            emergency_exit_count: None,
            paused_seconds: None,
            followed_by_break_type: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedAppEvent {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            study_mode_sync_id: None,
            focus_session_sync_id: None,
            package_name: None,
            app_name: None,
            event_type: None,
            action: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedChecklistTask {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedTodayPlanItem {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            today_date: None,
            source_task_sync_id: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            synced_source_completion: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedScheduleBlock {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            schedule_date: None,
            title: None,
            note: None,
            category_key: None,
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: None,
            start_minute: None,
            end_minute: None,
            status: None,
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedScheduleTemplate {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            title: None,
            note: None,
            category_key: None,
            subject_sync_id: None,
            weekdays: None,
            start_minute: None,
            end_minute: None,
            enabled: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedDailyReview {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            review_date: None,
            summary: None,
            blockers: None,
            tomorrow_focus: None,
            mood_score: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}

impl From<DeletedPayload> for SharedWeeklyReview {
    fn from(value: DeletedPayload) -> Self {
        Self {
            sync_id: value.sync_id,
            week_start_date: None,
            summary: None,
            blockers: None,
            next_week_focus: None,
            mood_score: None,
            created_at: None,
            updated_at: value.deleted_at.unwrap_or_default(),
            deleted_at: value.deleted_at,
        }
    }
}


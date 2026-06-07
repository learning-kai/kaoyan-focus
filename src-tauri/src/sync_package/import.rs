pub fn import_shared_sync_payload(
    connection: &mut Connection,
    payload: &SharedSyncPayload,
) -> Result<(), String> {
    set_primary_owner(
        connection,
        payload.primary_owner_device_id.as_deref(),
        payload.primary_owner_updated_at,
    )?;
    let mut payload = payload.clone();
    canonicalize_subject_payload(&mut payload);
    normalize_import_active_sessions(&mut payload);
    {
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        eprintln!("shared payload import: subjects {}", payload.subjects.len());
        import_subjects(&transaction, &payload.subjects)?;
        eprintln!(
            "shared payload import: checklist_tasks {}",
            payload.checklist_tasks.len()
        );
        import_checklist_tasks(&transaction, &payload.checklist_tasks)?;
        eprintln!(
            "shared payload import: today_plan_items {}",
            payload.today_plan_items.len()
        );
        import_today_plan_items(&transaction, &payload.today_plan_items)?;
        eprintln!(
            "shared payload import: schedule_templates {}",
            payload.schedule_templates.len()
        );
        import_schedule_templates(&transaction, &payload.schedule_templates)?;
        eprintln!(
            "shared payload import: schedule_blocks {}",
            payload.schedule_blocks.len()
        );
        import_schedule_blocks(&transaction, &payload.schedule_blocks)?;
        eprintln!(
            "shared payload import: daily_reviews {}",
            payload.daily_reviews.len()
        );
        import_daily_reviews(&transaction, &payload.daily_reviews)?;
        eprintln!(
            "shared payload import: weekly_reviews {}",
            payload.weekly_reviews.len()
        );
        import_weekly_reviews(&transaction, &payload.weekly_reviews)?;
        eprintln!("shared payload import: high priority commit");
        transaction.commit().map_err(|error| error.to_string())?;
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    eprintln!(
        "shared payload import: focus_sessions {}",
        payload.focus_sessions.len()
    );
    import_focus_sessions(&transaction, &payload.focus_sessions)?;
    eprintln!(
        "shared payload import: study_modes {}",
        payload.study_modes.len()
    );
    import_study_modes(&transaction, &payload.study_modes)?;
    eprintln!(
        "shared payload import: schedule_blocks second pass {}",
        payload.schedule_blocks.len()
    );
    import_schedule_blocks(&transaction, &payload.schedule_blocks)?;
    eprintln!(
        "shared payload import: app_events {}",
        payload.app_events.len()
    );
    import_app_events(&transaction, &payload.app_events)?;
    eprintln!("shared payload import: active_conflicts");
    resolve_local_active_conflicts(&transaction)?;
    eprintln!("shared payload import: commit");
    transaction.commit().map_err(|error| error.to_string())
}


fn import_subjects(connection: &Connection, items: &[SharedSubject]) -> Result<(), String> {
    for item in items {
        let sync_id = canonical_subject_sync_id_for_subject(&item.sync_id, item.name.as_deref());
        if let Some(deleted_at) = item.deleted_at {
            if is_default_subject_sync_id(&sync_id) {
                continue;
            }
            delete_local_row_by_sync_id(connection, ENTITY_SUBJECT, &sync_id, deleted_at)?;
            continue;
        }

        let Some(name) = item
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let enabled = item.enabled.unwrap_or(true);
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_subject_row(
            connection,
            &sync_id,
            name,
            item.color.clone(),
            enabled,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_study_modes(connection: &Connection, items: &[SharedStudyMode]) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(connection, ENTITY_STUDY_MODE, &item.sync_id, deleted_at)?;
            continue;
        }

        let Some(mode) = item
            .mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(phase) = item
            .phase
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(status) = item
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let current_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.current_session_sync_id.as_deref(),
        )?;
        let schedule_block_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            item.schedule_block_sync_id.as_deref(),
        )?;
        let today_plan_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.today_plan_item_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);
        let desktop_status = to_desktop_study_status(status);
        let desktop_phase = if phase == "paused" {
            item.paused_from_phase
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "paused")
                .unwrap_or("focus")
        } else {
            phase
        };
        let paused_at = if phase == "paused" || item.paused_at.is_some() {
            Some(millis_to_rfc3339(item.paused_at.unwrap_or(item.updated_at)))
        } else {
            None
        };

        upsert_study_mode_row(
            connection,
            &item.sync_id,
            item.state_revision.unwrap_or(1).max(1),
            mode,
            subject_id,
            item.planned_seconds.unwrap_or(0),
            item.focus_seconds.unwrap_or(0),
            item.break_seconds.unwrap_or(0),
            item.long_break_seconds.unwrap_or(900),
            item.long_break_interval.unwrap_or(4),
            desktop_phase,
            item.round_number.unwrap_or(1),
            &millis_to_rfc3339(item.started_at.unwrap_or(item.updated_at)),
            &millis_to_rfc3339(item.phase_started_at.unwrap_or(item.updated_at)),
            paused_at,
            item.accumulated_study_seconds.unwrap_or(0),
            item.total_paused_seconds.unwrap_or(0),
            item.paused_stage_elapsed_seconds
                .or(item.phase_paused_seconds)
                .unwrap_or(0),
            item.ended_at
                .as_ref()
                .map(|value| millis_to_rfc3339(*value)),
            current_session_id,
            schedule_block_id,
            today_plan_item_id,
            desktop_status,
            item.finish_reason.clone(),
            item.last_control_device_id.clone(),
            item.last_control_action.clone(),
            item.last_control_at,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_focus_sessions(
    connection: &Connection,
    items: &[SharedFocusSession],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_FOCUS_SESSION,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(mode) = item
            .mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(status) = item
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let schedule_block_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            item.schedule_block_sync_id.as_deref(),
        )?;
        let today_plan_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.today_plan_item_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_focus_session_row(
            connection,
            &item.sync_id,
            mode,
            subject_id,
            item.planned_seconds.unwrap_or(0),
            item.actual_seconds.unwrap_or(0),
            &millis_to_rfc3339(item.started_at.unwrap_or(item.updated_at)),
            item.ended_at
                .as_ref()
                .map(|value| millis_to_rfc3339(*value)),
            status,
            item.end_reason.clone(),
            item.interruption_count.unwrap_or(0),
            item.emergency_exit_count.unwrap_or(0),
            item.paused_seconds.unwrap_or(0),
            item.followed_by_break_type.clone(),
            schedule_block_id,
            today_plan_item_id,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_app_events(connection: &Connection, items: &[SharedAppEvent]) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(connection, ENTITY_APP_EVENT, &item.sync_id, deleted_at)?;
            continue;
        }

        let Some(package_name) = item
            .package_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(event_type) = item
            .event_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let focus_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.focus_session_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));

        upsert_app_event_row(
            connection,
            &item.sync_id,
            focus_session_id,
            package_name,
            item.app_name.clone(),
            event_type,
            item.action.clone(),
            &created_at,
        )?;
    }

    Ok(())
}

fn import_checklist_tasks(
    connection: &Connection,
    items: &[SharedChecklistTask],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_CHECKLIST_TASK,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(category_key) = item
            .category_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let board_scope = board_scope_for_category_key(category_key);
        ensure_checklist_column(connection, &board_scope)?;
        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let column_id = get_first_checklist_column_id(connection, &board_scope)?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_checklist_task_row(
            connection,
            &item.sync_id,
            &board_scope,
            subject_id,
            column_id,
            title,
            item.note.clone(),
            item.due_date.clone(),
            item.sort_order
                .map(|value| value.round() as i64)
                .unwrap_or(0),
            item.completed.unwrap_or(false),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_today_plan_items(
    connection: &Connection,
    items: &[SharedTodayPlanItem],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_TODAY_PLAN_ITEM,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(today_date) = item
            .today_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let source_task_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_CHECKLIST_TASK,
            item.source_task_sync_id.as_deref(),
        )?;
        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_today_plan_item_row(
            connection,
            &item.sync_id,
            today_date,
            source_task_id,
            subject_id,
            title,
            item.note.clone(),
            item.due_date.clone(),
            item.sort_order
                .map(|value| value.round() as i64)
                .unwrap_or(0),
            item.completed.unwrap_or(false),
            item.synced_source_completion.unwrap_or(false),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_schedule_templates(
    connection: &Connection,
    items: &[SharedScheduleTemplate],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_SCHEDULE_TEMPLATE,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);
        let weekdays = serde_json::to_string(&item.weekdays.clone().unwrap_or_default())
            .map_err(|error| error.to_string())?;

        upsert_schedule_template_row(
            connection,
            &item.sync_id,
            title,
            item.note.clone(),
            item.category_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("general"),
            subject_id,
            &weekdays,
            item.start_minute.unwrap_or(360),
            item.end_minute.unwrap_or(420),
            item.enabled.unwrap_or(true),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_schedule_blocks(
    connection: &Connection,
    items: &[SharedScheduleBlock],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_SCHEDULE_BLOCK,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(schedule_date) = item
            .schedule_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(title) = item
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let subject_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SUBJECT,
            item.subject_sync_id.as_deref(),
        )?;
        let source_today_item_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            item.source_today_item_sync_id.as_deref(),
        )?;
        let template_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            item.template_sync_id.as_deref(),
        )?;
        let linked_study_mode_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_STUDY_MODE,
            item.linked_study_mode_sync_id.as_deref(),
        )?;
        let linked_focus_session_id = resolve_existing_local_id_by_sync_id(
            connection,
            ENTITY_FOCUS_SESSION,
            item.linked_focus_session_sync_id.as_deref(),
        )?;
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_schedule_block_row(
            connection,
            &item.sync_id,
            schedule_date,
            title,
            item.note.clone(),
            item.category_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("general"),
            subject_id,
            source_today_item_id,
            template_id,
            item.start_minute.unwrap_or(360),
            item.end_minute.unwrap_or(420),
            item.status
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("planned"),
            linked_study_mode_id,
            linked_focus_session_id,
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_daily_reviews(
    connection: &Connection,
    items: &[SharedDailyReview],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_DAILY_REVIEW,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(review_date) = item
            .review_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_daily_review_row(
            connection,
            &item.sync_id,
            review_date,
            item.summary.clone(),
            item.blockers.clone(),
            item.tomorrow_focus.clone(),
            item.mood_score.unwrap_or(3).clamp(1, 5),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

fn import_weekly_reviews(
    connection: &Connection,
    items: &[SharedWeeklyReview],
) -> Result<(), String> {
    for item in items {
        if let Some(deleted_at) = item.deleted_at {
            delete_local_row_by_sync_id(
                connection,
                ENTITY_WEEKLY_REVIEW,
                &item.sync_id,
                deleted_at,
            )?;
            continue;
        }

        let Some(week_start_date) = item
            .week_start_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let created_at = millis_to_rfc3339(item.created_at.unwrap_or(item.updated_at));
        let updated_at = millis_to_rfc3339(item.updated_at);

        upsert_weekly_review_row(
            connection,
            &item.sync_id,
            week_start_date,
            item.summary.clone(),
            item.blockers.clone(),
            item.next_week_focus.clone(),
            item.mood_score.unwrap_or(3).clamp(1, 5),
            &created_at,
            &updated_at,
        )?;
    }

    Ok(())
}

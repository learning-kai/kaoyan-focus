fn trigger_shared_sync(app: &AppHandle, trigger: &'static str) {
    let sync_app = app.clone();
    thread::spawn(move || {
        let _ = crate::commands::sync::sync_object_storage_after_external_change(sync_app, trigger);
    });
    crate::commands::feishu::sync_feishu_bridge_after_local_change(app.clone(), trigger);
}

pub fn sync_study_runtime_state(app: &AppHandle) -> Result<bool, String> {
    let connection = open_database(&database_path(app)?)?;
    let state = app.state::<AppState>();
    if let Some(record) = get_active_study_mode_record(&connection)? {
        set_runtime_state(state.inner(), true, record.current_session_id)?;
        return Ok(record.paused_at.is_none());
    } else {
        set_runtime_state(state.inner(), false, None)?;
    }

    Ok(false)
}

pub(crate) fn sync_focus_widget_for_state(app: &AppHandle, state: &StudyModeState) {
    let _ = crate::windows::focus_widget::sync_visibility_with_study_mode_state(app, state);
}

fn current_study_runtime_marker(app: &AppHandle) -> Result<Option<StudyRuntimeSyncMarker>, String> {
    let connection = open_database(&database_path(app)?)?;
    let state = load_current_study_mode_state(&connection, Utc::now())?;
    Ok(study_runtime_marker(&state))
}

fn study_runtime_marker(state: &StudyModeState) -> Option<StudyRuntimeSyncMarker> {
    if state.id.is_none() && state.status == "idle" {
        return None;
    }

    Some(StudyRuntimeSyncMarker {
        id: state.id,
        state_revision: state.state_revision.unwrap_or(0).max(0),
        phase: state.phase.clone(),
        status: state.status.clone(),
        subject_id: state.subject_id,
        cycle_index: state.cycle_index,
        paused_at: state.paused_at.clone(),
        current_session_id: state.current_session.as_ref().map(|session| session.id),
        break_kind: state.break_kind.clone(),
    })
}


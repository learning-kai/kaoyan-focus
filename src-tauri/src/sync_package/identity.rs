pub fn mark_entity_deleted(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    deleted_at: i64,
) -> Result<(), String> {
    let sync_id = resolve_or_create_sync_id(connection, entity_type, local_id, None, deleted_at)?;
    upsert_sync_meta(
        connection,
        entity_type,
        local_id,
        &sync_id,
        deleted_at,
        Some(deleted_at),
    )?;
    Ok(())
}

fn merge_latest_by_sync_id<T, Id, Updated, Deleted>(
    local: &[T],
    remote: &[T],
    id_of: Id,
    updated_at_of: Updated,
    deleted_at_of: Deleted,
) -> Vec<T>
where
    T: Clone,
    Id: Fn(&T) -> &str,
    Updated: Fn(&T) -> i64,
    Deleted: Fn(&T) -> Option<i64>,
{
    let mut merged: Vec<T> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = id_of(item).trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            let current = &merged[position];
            let item_updated_at = updated_at_of(item);
            let current_updated_at = updated_at_of(current);
            let tombstone_tie_wins = item_updated_at == current_updated_at
                && deleted_at_of(item).is_some()
                && deleted_at_of(current).is_none();
            if item_updated_at > current_updated_at || tombstone_tie_wins {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }

    merged
}

fn merge_study_modes(
    local: &[SharedStudyMode],
    remote: &[SharedStudyMode],
) -> Vec<SharedStudyMode> {
    let mut merged: Vec<SharedStudyMode> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = item.sync_id.trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            if should_replace_study_mode(&merged[position], item) {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }
    merged
}

fn should_replace_study_mode(existing: &SharedStudyMode, candidate: &SharedStudyMode) -> bool {
    let existing_deleted = existing.deleted_at.is_some();
    let candidate_deleted = candidate.deleted_at.is_some();
    if candidate.updated_at == existing.updated_at && candidate_deleted && !existing_deleted {
        return true;
    }

    let existing_revision = existing.state_revision.unwrap_or(0).max(0);
    let candidate_revision = candidate.state_revision.unwrap_or(0).max(0);
    if candidate_revision != existing_revision {
        return candidate_revision > existing_revision;
    }

    if existing_deleted != candidate_deleted {
        return candidate_deleted && candidate.updated_at >= existing.updated_at;
    }

    candidate.updated_at > existing.updated_at
}

fn merge_focus_sessions(
    local: &[SharedFocusSession],
    remote: &[SharedFocusSession],
) -> Vec<SharedFocusSession> {
    let mut merged: Vec<SharedFocusSession> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for item in local.iter().chain(remote.iter()) {
        let sync_id = item.sync_id.trim();
        if sync_id.is_empty() {
            continue;
        }

        if let Some(position) = positions.get(sync_id).copied() {
            if should_replace_focus_session(&merged[position], item) {
                merged[position] = item.clone();
            }
        } else {
            positions.insert(sync_id.to_string(), merged.len());
            merged.push(item.clone());
        }
    }
    merged
}

fn should_replace_focus_session(
    existing: &SharedFocusSession,
    candidate: &SharedFocusSession,
) -> bool {
    let existing_deleted = existing.deleted_at.is_some();
    let candidate_deleted = candidate.deleted_at.is_some();
    if candidate.updated_at == existing.updated_at && candidate_deleted && !existing_deleted {
        return true;
    }

    let existing_running = existing.status.as_deref() == Some("running");
    let candidate_running = candidate.status.as_deref() == Some("running");
    if existing_running != candidate_running {
        return !candidate_running && candidate.ended_at.is_some();
    }

    if existing_deleted != candidate_deleted {
        return candidate_deleted && candidate.updated_at >= existing.updated_at;
    }

    candidate.updated_at > existing.updated_at
}

fn active_sort_key(mode: &SharedStudyMode) -> (i64, i64) {
    (
        mode.state_revision.unwrap_or(0).max(0),
        mode.updated_at.max(0),
    )
}

const ACTIVE_CONTROL_ACTIONS: [&str; 6] = [
    "pause",
    "resume",
    "confirm_break",
    "finish",
    "emergency_exit",
    "switch_subject",
];

#[derive(Debug, Clone, Copy)]
struct ActiveLogicalPosition {
    round: i64,
    phase_rank: i64,
    progress_seconds: i64,
}

#[derive(Debug, Clone)]
struct DbStudyModeProgress {
    phase: String,
    round_number: i64,
    phase_started_at: Option<i64>,
    paused_at: Option<i64>,
    accumulated_study_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    status: String,
    ended_at: Option<i64>,
    updated_at: i64,
}

fn merge_primary_owner(
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
) -> (Option<String>, Option<i64>) {
    let local_updated_at = local.primary_owner_updated_at.unwrap_or(0).max(0);
    let remote_updated_at = remote.primary_owner_updated_at.unwrap_or(0).max(0);
    if remote_updated_at > local_updated_at {
        (
            remote.primary_owner_device_id.clone(),
            Some(remote_updated_at),
        )
    } else if local_updated_at > 0 {
        (
            local.primary_owner_device_id.clone(),
            Some(local_updated_at),
        )
    } else if remote.primary_owner_device_id.is_some() {
        (
            remote.primary_owner_device_id.clone(),
            remote.primary_owner_updated_at,
        )
    } else {
        (
            local.primary_owner_device_id.clone(),
            local.primary_owner_updated_at,
        )
    }
}

fn primary_owner_prefers_local(
    primary_owner_device_id: Option<&str>,
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    remote_active: Option<&SharedActiveStudySnapshot>,
) -> bool {
    let Some(owner) = primary_owner_device_id.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    let Some(local_active) = local_active else {
        return false;
    };
    if local.device_id != owner {
        return false;
    }
    if remote.device_id == owner {
        return false;
    }
    remote_active
        .map(|snapshot| snapshot.sync_id.as_str() != local_active.sync_id.as_str())
        .unwrap_or(false)
}

fn should_keep_local_active(
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    remote_active: Option<&SharedActiveStudySnapshot>,
    now_millis: i64,
) -> bool {
    let (Some(local_active), Some(remote_active)) = (local_active, remote_active) else {
        return false;
    };

    let local_revision = local_active.state_revision.unwrap_or(0).max(0);
    let remote_revision = remote_active.state_revision.unwrap_or(0).max(0);
    if local_revision != remote_revision {
        return local_revision > remote_revision;
    }

    if local_active.updated_at >= remote_active.updated_at {
        return true;
    }

    if local_active.sync_id != remote_active.sync_id {
        return false;
    }

    let local_mode = local
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id);
    let remote_mode = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == remote_active.sync_id);

    match (local_mode, remote_mode) {
        (Some(local_mode), Some(remote_mode)) => {
            active_mode_regresses(local_mode, remote_mode, now_millis)
        }
        _ => false,
    }
}

fn classify_active_remote_merge(
    local: &SharedSyncPayload,
    remote: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    now_millis: i64,
    primary_owner_device_id: Option<&str>,
) -> ActiveRemoteMergeDecision {
    let Some(local_active) = local_active else {
        return ActiveRemoteMergeDecision::Default;
    };
    let Some(local_mode) = local
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
    else {
        return ActiveRemoteMergeDecision::Default;
    };
    let Some(remote_mode) = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
    else {
        if shared_active_study_snapshot(remote).is_some() {
            return ActiveRemoteMergeDecision::KeepLocalActive;
        }
        return ActiveRemoteMergeDecision::Default;
    };

    let local_is_primary = primary_owner_device_id
        .map(|owner| !owner.trim().is_empty() && local.device_id == owner)
        .unwrap_or(false);
    let remote_is_primary = primary_owner_device_id
        .map(|owner| !owner.trim().is_empty() && remote.device_id == owner)
        .unwrap_or(false);
    let valid_control = is_remote_active_command(local_mode, remote_mode)
        && has_valid_remote_control_intent(local_mode, remote_mode, &remote.device_id);
    if local_is_primary && !remote_is_primary {
        return if valid_control {
            ActiveRemoteMergeDecision::AcceptRemoteCommand
        } else {
            ActiveRemoteMergeDecision::KeepLocalActive
        };
    }

    if remote_mode.updated_at <= local_mode.updated_at {
        return if active_mode_regresses(local_mode, remote_mode, now_millis) {
            ActiveRemoteMergeDecision::KeepLocalActive
        } else {
            ActiveRemoteMergeDecision::Default
        };
    }

    if valid_control {
        return ActiveRemoteMergeDecision::AcceptRemoteCommand;
    }

    if active_mode_regresses(local_mode, remote_mode, now_millis) {
        return ActiveRemoteMergeDecision::KeepLocalActive;
    }

    ActiveRemoteMergeDecision::Default
}

fn is_remote_active_command(local: &SharedStudyMode, remote: &SharedStudyMode) -> bool {
    let remote_status = remote.status.as_deref().unwrap_or_default();
    let remote_phase = remote.phase.as_deref().unwrap_or_default();
    if matches!(remote_status, "finished" | "emergency_exited")
        || matches!(remote_phase, "finished" | "emergency_exited")
        || remote.ended_at.is_some()
    {
        return true;
    }

    let local_phase = local.phase.as_deref().unwrap_or_default();
    let local_paused = local_phase == "paused" || local.paused_at.is_some();
    let remote_paused = remote_phase == "paused" || remote.paused_at.is_some();
    if remote_paused && !local_paused {
        return true;
    }
    if local_paused && !remote_paused && matches!(remote_phase, "focus" | "awaiting_break") {
        return true;
    }
    local_phase == "awaiting_break" && remote_phase == "break"
}

fn has_valid_remote_control_intent(
    local: &SharedStudyMode,
    remote: &SharedStudyMode,
    remote_device_id: &str,
) -> bool {
    let Some(control_device_id) = remote
        .last_control_device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if control_device_id != remote_device_id {
        return false;
    }
    let Some(action) = remote
        .last_control_action
        .as_deref()
        .map(str::trim)
        .filter(|value| ACTIVE_CONTROL_ACTIONS.contains(value))
    else {
        return false;
    };
    let Some(control_at) = remote.last_control_at.filter(|value| *value > 0) else {
        return false;
    };
    if remote.updated_at != control_at {
        return false;
    }
    if local
        .last_control_at
        .filter(|accepted_at| control_at < *accepted_at)
        .is_some()
    {
        return false;
    }
    control_action_matches_state(action, local, remote)
}

fn control_action_matches_state(
    action: &str,
    local: &SharedStudyMode,
    remote: &SharedStudyMode,
) -> bool {
    let local_phase = local.phase.as_deref().unwrap_or_default();
    let remote_phase = remote.phase.as_deref().unwrap_or_default();
    let local_paused = local_phase == "paused" || local.paused_at.is_some();
    let remote_paused = remote_phase == "paused" || remote.paused_at.is_some();
    match action {
        "pause" => remote_paused && !local_paused,
        "resume" => {
            local_paused && !remote_paused && matches!(remote_phase, "focus" | "awaiting_break")
        }
        "confirm_break" => local_phase == "awaiting_break" && remote_phase == "break",
        "finish" => {
            remote.status.as_deref() == Some("finished")
                || remote_phase == "finished"
                || (remote.ended_at.is_some()
                    && remote.status.as_deref() != Some("emergency_exited"))
        }
        "emergency_exit" => {
            remote.status.as_deref() == Some("emergency_exited")
                || remote_phase == "emergency_exited"
        }
        "switch_subject" => local.subject_sync_id != remote.subject_sync_id,
        _ => false,
    }
}

fn active_mode_regresses(
    local: &SharedStudyMode,
    remote: &SharedStudyMode,
    now_millis: i64,
) -> bool {
    let local_position = active_logical_position(local, now_millis);
    let remote_position = active_logical_position(remote, now_millis);

    if remote_position.round != local_position.round {
        return remote_position.round < local_position.round;
    }
    if remote_position.phase_rank != local_position.phase_rank {
        return remote_position.phase_rank < local_position.phase_rank;
    }

    remote_position.progress_seconds + 5 < local_position.progress_seconds
}

fn active_logical_position(mode: &SharedStudyMode, now_millis: i64) -> ActiveLogicalPosition {
    let round = mode.round_number.unwrap_or(1).max(1);
    let phase = effective_shared_phase(mode);
    let phase_rank = match phase.as_str() {
        "awaiting_break" => 1,
        "break" => 2,
        "finished" | "emergency_exited" => 3,
        _ => 0,
    };
    let accumulated = mode.accumulated_study_seconds.unwrap_or(0).max(0);
    let planned = mode.planned_seconds.unwrap_or(i64::MAX / 4).max(0);
    let remaining = planned.saturating_sub(accumulated);
    let current_focus_seconds = if phase == "focus" {
        let focus_seconds = mode.focus_seconds.unwrap_or(remaining).max(0);
        phase_elapsed_seconds(mode, now_millis)
            .min(focus_seconds)
            .min(remaining)
            .max(0)
    } else {
        0
    };

    ActiveLogicalPosition {
        round,
        phase_rank,
        progress_seconds: (accumulated + current_focus_seconds).min(planned).max(0),
    }
}

fn effective_shared_phase(mode: &SharedStudyMode) -> String {
    let phase = mode.phase.as_deref().unwrap_or("focus");
    if phase == "paused" {
        mode.paused_from_phase
            .as_deref()
            .filter(|value| !value.trim().is_empty() && *value != "paused")
            .unwrap_or("focus")
            .to_string()
    } else {
        phase.to_string()
    }
}

fn phase_elapsed_seconds(mode: &SharedStudyMode, now_millis: i64) -> i64 {
    if mode.phase.as_deref() == Some("paused") || mode.paused_at.is_some() {
        return mode
            .paused_stage_elapsed_seconds
            .or(mode.phase_paused_seconds)
            .unwrap_or(0)
            .max(0);
    }

    mode.phase_started_at
        .map(|started_at| ((now_millis - started_at).max(0)) / 1000)
        .unwrap_or(0)
}

fn restore_active_from_payload(
    study_modes: &mut Vec<SharedStudyMode>,
    focus_sessions: &mut Vec<SharedFocusSession>,
    source: &SharedSyncPayload,
    snapshot: &SharedActiveStudySnapshot,
) {
    let Some(source_mode) = source
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == snapshot.sync_id)
        .cloned()
    else {
        return;
    };

    if let Some(position) = study_modes
        .iter()
        .position(|mode| mode.sync_id == source_mode.sync_id)
    {
        study_modes[position] = source_mode.clone();
    } else {
        study_modes.push(source_mode.clone());
    }

    if let Some(session_sync_id) = source_mode.current_session_sync_id.as_deref() {
        if let Some(source_session) = source
            .focus_sessions
            .iter()
            .find(|session| session.sync_id == session_sync_id)
            .cloned()
        {
            if let Some(position) = focus_sessions
                .iter()
                .position(|session| session.sync_id == source_session.sync_id)
            {
                focus_sessions[position] = source_session;
            } else {
                focus_sessions.push(source_session);
            }
        }
    }
}

fn restore_matching_active_from_remote(
    study_modes: &mut Vec<SharedStudyMode>,
    focus_sessions: &mut Vec<SharedFocusSession>,
    remote: &SharedSyncPayload,
    local: &SharedSyncPayload,
    local_active: Option<&SharedActiveStudySnapshot>,
    now_millis: i64,
) {
    let Some(local_active) = local_active else {
        return;
    };
    let Some(mut remote_mode) = remote
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
        .cloned()
    else {
        return;
    };
    if let Some(local_mode) = local
        .study_modes
        .iter()
        .find(|mode| mode.sync_id == local_active.sync_id)
    {
        remote_mode = preserve_active_progress_for_command(local_mode, remote_mode, now_millis);
    }

    if let Some(position) = study_modes
        .iter()
        .position(|mode| mode.sync_id == remote_mode.sync_id)
    {
        study_modes[position] = remote_mode.clone();
    } else {
        study_modes.push(remote_mode.clone());
    }

    if let Some(session_sync_id) = remote_mode.current_session_sync_id.as_deref() {
        if let Some(remote_session) = remote
            .focus_sessions
            .iter()
            .find(|session| session.sync_id == session_sync_id)
            .cloned()
        {
            if let Some(position) = focus_sessions
                .iter()
                .position(|session| session.sync_id == remote_session.sync_id)
            {
                focus_sessions[position] = remote_session;
            } else {
                focus_sessions.push(remote_session);
            }
        }
    }
}

fn preserve_active_progress_for_command(
    local: &SharedStudyMode,
    mut remote: SharedStudyMode,
    now_millis: i64,
) -> SharedStudyMode {
    let local_position = active_logical_position(local, now_millis);
    let remote_position = active_logical_position(&remote, now_millis);
    remote.round_number = Some(
        remote
            .round_number
            .unwrap_or(1)
            .max(local.round_number.unwrap_or(1)),
    );
    remote.accumulated_study_seconds = Some(
        remote
            .accumulated_study_seconds
            .unwrap_or(0)
            .max(local.accumulated_study_seconds.unwrap_or(0)),
    );
    if remote_position.progress_seconds < local_position.progress_seconds {
        match remote.phase.as_deref() {
            Some("paused") => {
                remote.paused_stage_elapsed_seconds = Some(
                    remote
                        .paused_stage_elapsed_seconds
                        .or(remote.phase_paused_seconds)
                        .unwrap_or(0)
                        .max(phase_elapsed_seconds(local, now_millis)),
                );
                remote.phase_paused_seconds = remote.paused_stage_elapsed_seconds;
            }
            Some("focus") => {
                if let Some(local_started_at) = local.phase_started_at {
                    remote.phase_started_at = Some(
                        remote
                            .phase_started_at
                            .unwrap_or(local_started_at)
                            .min(local_started_at),
                    );
                }
            }
            _ => {
                remote.accumulated_study_seconds = Some(
                    remote
                        .accumulated_study_seconds
                        .unwrap_or(0)
                        .max(local_position.progress_seconds),
                );
            }
        }
    }
    remote
}

fn resolve_shared_active_conflicts(
    study_modes: &mut [SharedStudyMode],
    focus_sessions: &mut [SharedFocusSession],
    resolved_at: i64,
    preferred_winner_sync_id: Option<&str>,
) {
    let preferred_winner_sync_id = preferred_winner_sync_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let winner_sync_id = preferred_winner_sync_id
        .and_then(|preferred| {
            study_modes
                .iter()
                .find(|item| {
                    item.deleted_at.is_none()
                        && item.sync_id == preferred
                        && item
                            .status
                            .as_deref()
                            .map(is_shared_running_status)
                            .unwrap_or(false)
                })
                .map(|item| item.sync_id.clone())
        })
        .or_else(|| {
            study_modes
                .iter()
                .filter(|item| item.deleted_at.is_none())
                .filter(|item| {
                    item.status
                        .as_deref()
                        .map(is_shared_running_status)
                        .unwrap_or(false)
                })
                .max_by_key(|item| active_sort_key(item))
                .map(|item| item.sync_id.clone())
        });

    let Some(winner_sync_id) = winner_sync_id else {
        return;
    };

    let mut losing_mode_ids = HashSet::new();
    let mut losing_session_ids = HashSet::new();

    for mode in study_modes.iter_mut() {
        let is_running = mode
            .status
            .as_deref()
            .map(is_shared_running_status)
            .unwrap_or(false);
        if !is_running || mode.sync_id == winner_sync_id {
            continue;
        }

        losing_mode_ids.insert(mode.sync_id.clone());
        if let Some(session_sync_id) = mode.current_session_sync_id.as_deref() {
            losing_session_ids.insert(session_sync_id.to_string());
        }

        mode.status = Some("finished".to_string());
        mode.phase = Some("finished".to_string());
        mode.ended_at = mode.ended_at.or(Some(resolved_at));
        mode.current_session_sync_id = None;
        mode.finish_reason = Some("sync_takeover".to_string());
        mode.updated_at = mode.updated_at.max(resolved_at);
    }

    if losing_mode_ids.is_empty() && losing_session_ids.is_empty() {
        return;
    }

    for session in focus_sessions.iter_mut() {
        let belongs_to_losing_mode = session
            .study_mode_sync_id
            .as_deref()
            .map(|sync_id| losing_mode_ids.contains(sync_id))
            .unwrap_or(false);
        let is_losing_current_session = losing_session_ids.contains(&session.sync_id);
        let is_running = session.status.as_deref() == Some("running");

        if is_running && (belongs_to_losing_mode || is_losing_current_session) {
            session.status = Some("finished".to_string());
            session.ended_at = session.ended_at.or(Some(resolved_at));
            session.end_reason = Some("sync_takeover".to_string());
            session.updated_at = session.updated_at.max(resolved_at);
        }
    }
}

fn is_shared_running_status(status: &str) -> bool {
    status == "running" || status == "active"
}

fn to_shared_study_status(status: &str) -> &str {
    if status == "active" {
        "running"
    } else {
        status
    }
}

fn to_desktop_study_status(status: &str) -> &str {
    if status == "running" {
        "active"
    } else {
        status
    }
}

fn break_type_after_round(round_number: i64, long_break_interval: i64) -> String {
    if long_break_interval > 0 && round_number > 0 && round_number % long_break_interval == 0 {
        "long".to_string()
    } else {
        "short".to_string()
    }
}


fn load_subject_rows(connection: &Connection) -> Result<Vec<DesktopSubjectRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, name, color, enabled, created_at, updated_at
            FROM subjects
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopSubjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_study_mode_rows(connection: &Connection) -> Result<Vec<DesktopStudyModeRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
                   long_break_seconds, long_break_interval, phase, cycle_index,
                   started_at, phase_started_at, paused_at, total_paused_seconds,
                   phase_paused_seconds, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, ended_at, current_session_id, status,
                   finish_reason, created_at, updated_at, schedule_block_id, today_plan_item_id,
                   last_control_device_id, last_control_action, last_control_at
            FROM study_modes
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopStudyModeRow {
                id: row.get(0)?,
                state_revision: row.get(1)?,
                mode: row.get(2)?,
                subject_id: row.get(3)?,
                planned_seconds: row.get(4)?,
                focus_seconds: row.get(5)?,
                break_seconds: row.get(6)?,
                long_break_seconds: row.get(7)?,
                long_break_interval: row.get(8)?,
                phase: row.get(9)?,
                cycle_index: row.get(10)?,
                started_at: row.get(11)?,
                phase_started_at: row.get(12)?,
                paused_at: row.get(13)?,
                total_paused_seconds: row.get(14)?,
                _phase_paused_seconds: row.get(15)?,
                accumulated_study_seconds: row.get(16)?,
                paused_stage_elapsed_seconds: row.get(17)?,
                ended_at: row.get(18)?,
                current_session_id: row.get(19)?,
                status: row.get(20)?,
                finish_reason: row.get(21)?,
                created_at: row.get(22)?,
                updated_at: row.get(23)?,
                schedule_block_id: row.get(24)?,
                today_plan_item_id: row.get(25)?,
                last_control_device_id: row.get(26)?,
                last_control_action: row.get(27)?,
                last_control_at: row.get(28)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_focus_session_rows(connection: &Connection) -> Result<Vec<DesktopFocusSessionRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
                   status, end_reason, interruption_count, emergency_exit_count, paused_seconds,
                   followed_by_break_type, created_at, updated_at, schedule_block_id, today_plan_item_id
            FROM focus_sessions
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopFocusSessionRow {
                id: row.get(0)?,
                mode: row.get(1)?,
                subject_id: row.get(2)?,
                planned_seconds: row.get(3)?,
                actual_seconds: row.get(4)?,
                started_at: row.get(5)?,
                ended_at: row.get(6)?,
                status: row.get(7)?,
                end_reason: row.get(8)?,
                interruption_count: row.get(9)?,
                emergency_exit_count: row.get(10)?,
                paused_seconds: row.get(11)?,
                followed_by_break_type: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
                schedule_block_id: row.get(15)?,
                today_plan_item_id: row.get(16)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_app_event_rows(connection: &Connection) -> Result<Vec<DesktopAppEventRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, session_id, process_name, window_title, event_type, action_taken, created_at
            FROM app_events
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopAppEventRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                process_name: row.get(2)?,
                window_title: row.get(3)?,
                event_type: row.get(4)?,
                action_taken: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_checklist_task_rows(
    connection: &Connection,
) -> Result<Vec<DesktopChecklistTaskRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, board_scope, subject_id, title, note, due_date, sort_order, completed, created_at, updated_at
            FROM checklist_tasks
            WHERE completed IN (0, 1)
            ORDER BY id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopChecklistTaskRow {
                id: row.get(0)?,
                board_scope: row.get(1)?,
                subject_id: row.get(2)?,
                title: row.get(3)?,
                note: row.get(4)?,
                due_date: row.get(5)?,
                sort_order: row.get(6)?,
                completed: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_today_plan_item_rows(
    connection: &Connection,
) -> Result<Vec<DesktopTodayPlanItemRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            FROM today_plan_items
            ORDER BY today_date ASC, sort_order ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopTodayPlanItemRow {
                id: row.get(0)?,
                today_date: row.get(1)?,
                source_task_id: row.get(2)?,
                subject_id: row.get(3)?,
                title: row.get(4)?,
                note: row.get(5)?,
                due_date: row.get(6)?,
                sort_order: row.get(7)?,
                completed: row.get::<_, i64>(8)? != 0,
                synced_source_completion: row.get::<_, i64>(9)? != 0,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_schedule_block_rows(
    connection: &Connection,
) -> Result<Vec<DesktopScheduleBlockRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, schedule_date, title, note, category_key, subject_id, source_today_item_id,
                   template_id, start_minute, end_minute, status, linked_study_mode_id,
                   linked_focus_session_id, created_at, updated_at
            FROM schedule_blocks
            ORDER BY schedule_date ASC, start_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopScheduleBlockRow {
                id: row.get(0)?,
                schedule_date: row.get(1)?,
                title: row.get(2)?,
                note: row.get(3)?,
                category_key: row.get(4)?,
                subject_id: row.get(5)?,
                source_today_item_id: row.get(6)?,
                template_id: row.get(7)?,
                start_minute: row.get(8)?,
                end_minute: row.get(9)?,
                status: row.get(10)?,
                linked_study_mode_id: row.get(11)?,
                linked_focus_session_id: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_schedule_template_rows(
    connection: &Connection,
) -> Result<Vec<DesktopScheduleTemplateRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, note, category_key, subject_id, weekdays, start_minute,
                   end_minute, enabled, created_at, updated_at
            FROM schedule_templates
            ORDER BY start_minute ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopScheduleTemplateRow {
                id: row.get(0)?,
                title: row.get(1)?,
                note: row.get(2)?,
                category_key: row.get(3)?,
                subject_id: row.get(4)?,
                weekdays: row.get(5)?,
                start_minute: row.get(6)?,
                end_minute: row.get(7)?,
                enabled: row.get::<_, i64>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_daily_review_rows(connection: &Connection) -> Result<Vec<DesktopDailyReviewRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            FROM daily_reviews
            ORDER BY review_date ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopDailyReviewRow {
                id: row.get(0)?,
                review_date: row.get(1)?,
                summary: row.get(2)?,
                blockers: row.get(3)?,
                tomorrow_focus: row.get(4)?,
                mood_score: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_weekly_review_rows(connection: &Connection) -> Result<Vec<DesktopWeeklyReviewRow>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            FROM weekly_reviews
            ORDER BY week_start_date ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(DesktopWeeklyReviewRow {
                id: row.get(0)?,
                week_start_date: row.get(1)?,
                summary: row.get(2)?,
                blockers: row.get(3)?,
                next_week_focus: row.get(4)?,
                mood_score: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn ensure_checklist_column(connection: &Connection, board_scope: &str) -> Result<(), String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM checklist_columns WHERE board_scope = ?1",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    if count > 0 {
        return Ok(());
    }

    let now = millis_to_rfc3339(Utc::now().timestamp_millis());
    connection
        .execute(
            "
            INSERT INTO checklist_columns (board_scope, name, sort_order, created_at, updated_at)
            VALUES (?1, 'Default', 0, ?2, ?2)
            ",
            params![board_scope, now],
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn get_first_checklist_column_id(
    connection: &Connection,
    board_scope: &str,
) -> Result<i64, String> {
    connection
        .query_row(
            "
            SELECT id
            FROM checklist_columns
            WHERE board_scope = ?1
            ORDER BY sort_order ASC, id ASC
            LIMIT 1
            ",
            params![board_scope],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn upsert_subject_row(
    connection: &Connection,
    sync_id: &str,
    name: &str,
    color: Option<String>,
    enabled: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) = resolve_local_id_by_sync_id(connection, ENTITY_SUBJECT, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE subjects
                SET name = ?1,
                    color = ?2,
                    enabled = ?3,
                    created_at = ?4,
                    updated_at = ?5
                WHERE id = ?6
                ",
                params![
                    name,
                    color,
                    if enabled { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SUBJECT,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO subjects (name, color, enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                name,
                color,
                if enabled { 1 } else { 0 },
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SUBJECT,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_study_mode_row(
    connection: &Connection,
    sync_id: &str,
    state_revision: i64,
    mode: &str,
    subject_id: Option<i64>,
    planned_seconds: i64,
    focus_seconds: i64,
    break_seconds: i64,
    long_break_seconds: i64,
    long_break_interval: i64,
    phase: &str,
    round_number: i64,
    started_at: &str,
    phase_started_at: &str,
    paused_at: Option<String>,
    accumulated_study_seconds: i64,
    total_paused_seconds: i64,
    paused_stage_elapsed_seconds: i64,
    ended_at: Option<String>,
    current_session_id: Option<i64>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    status: &str,
    finish_reason: Option<String>,
    last_control_device_id: Option<String>,
    last_control_action: Option<String>,
    last_control_at: Option<i64>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let incoming_updated_at_millis = parse_rfc3339_millis(updated_at)?;
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_STUDY_MODE, Some(sync_id))?
    {
        let incoming = DbStudyModeProgress {
            phase: phase.to_string(),
            round_number,
            phase_started_at: parse_rfc3339_millis(phase_started_at).ok(),
            paused_at: paused_at
                .as_deref()
                .and_then(|value| parse_rfc3339_millis(value).ok()),
            accumulated_study_seconds,
            paused_stage_elapsed_seconds,
            status: status.to_string(),
            ended_at: ended_at
                .as_deref()
                .and_then(|value| parse_rfc3339_millis(value).ok()),
            updated_at: incoming_updated_at_millis,
        };
        let existing = load_db_study_mode_progress(connection, local_id)?;
        let keep_existing_progress = existing.as_ref().is_some_and(|existing| {
            should_preserve_db_active_progress(existing, &incoming, incoming_updated_at_millis)
        });

        let (
            next_phase,
            next_round_number,
            next_phase_started_at,
            next_paused_at,
            next_accumulated_study_seconds,
            next_paused_stage_elapsed_seconds,
            next_status,
            next_ended_at,
        ) = if keep_existing_progress {
            let existing = existing.expect("existing progress checked");
            (
                existing.phase,
                existing.round_number,
                existing
                    .phase_started_at
                    .map(millis_to_rfc3339)
                    .unwrap_or_else(|| phase_started_at.to_string()),
                existing.paused_at.map(millis_to_rfc3339),
                existing.accumulated_study_seconds,
                existing.paused_stage_elapsed_seconds,
                existing.status,
                existing.ended_at.map(millis_to_rfc3339),
            )
        } else {
            (
                phase.to_string(),
                round_number,
                phase_started_at.to_string(),
                paused_at,
                accumulated_study_seconds,
                paused_stage_elapsed_seconds,
                status.to_string(),
                ended_at,
            )
        };
        connection
            .execute(
                "
                UPDATE study_modes
                SET state_revision = ?1,
                    mode = ?2,
                    subject_id = ?3,
                    planned_seconds = ?4,
                    focus_seconds = ?5,
                    break_seconds = ?6,
                    long_break_seconds = ?7,
                    long_break_interval = ?8,
                    phase = ?9,
                    cycle_index = ?10,
                    started_at = ?11,
                    phase_started_at = ?12,
                    paused_at = ?13,
                    accumulated_study_seconds = ?14,
                    total_paused_seconds = ?15,
                    phase_paused_seconds = ?16,
                    paused_stage_elapsed_seconds = ?16,
                    ended_at = ?17,
                    current_session_id = ?18,
                    schedule_block_id = ?19,
                    today_plan_item_id = ?20,
                    status = ?21,
                    finish_reason = ?22,
                    last_control_device_id = ?23,
                    last_control_action = ?24,
                    last_control_at = ?25,
                    created_at = ?26,
                    updated_at = ?27
                WHERE id = ?28
                ",
                params![
                    state_revision.max(1),
                    mode,
                    subject_id,
                    planned_seconds,
                    focus_seconds,
                    break_seconds,
                    long_break_seconds,
                    long_break_interval,
                    next_phase,
                    next_round_number,
                    started_at,
                    next_phase_started_at,
                    next_paused_at,
                    next_accumulated_study_seconds,
                    total_paused_seconds,
                    next_paused_stage_elapsed_seconds,
                    next_ended_at,
                    current_session_id,
                    schedule_block_id,
                    today_plan_item_id,
                    next_status,
                    finish_reason,
                    last_control_device_id,
                    last_control_action,
                    last_control_at,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_STUDY_MODE,
            local_id,
            sync_id,
            incoming_updated_at_millis,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO study_modes (
              state_revision, mode, subject_id, planned_seconds, focus_seconds, break_seconds,
              long_break_seconds, long_break_interval, phase, cycle_index,
              started_at, phase_started_at, paused_at, accumulated_study_seconds,
              total_paused_seconds, phase_paused_seconds, paused_stage_elapsed_seconds,
              ended_at, current_session_id, schedule_block_id,
              today_plan_item_id, status, finish_reason, last_control_device_id,
              last_control_action, last_control_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
            ",
            params![
                state_revision.max(1),
                mode,
                subject_id,
                planned_seconds,
                focus_seconds,
                break_seconds,
                long_break_seconds,
                long_break_interval,
                phase,
                round_number,
                started_at,
                phase_started_at,
                paused_at,
                accumulated_study_seconds,
                total_paused_seconds,
                paused_stage_elapsed_seconds,
                paused_stage_elapsed_seconds,
                ended_at,
                current_session_id,
                schedule_block_id,
                today_plan_item_id,
                status,
                finish_reason,
                last_control_device_id,
                last_control_action,
                last_control_at,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_STUDY_MODE,
        local_id,
        sync_id,
        incoming_updated_at_millis,
        None,
    )
}

fn load_db_study_mode_progress(
    connection: &Connection,
    local_id: i64,
) -> Result<Option<DbStudyModeProgress>, String> {
    connection
        .query_row(
            "
            SELECT phase, cycle_index, phase_started_at, paused_at, accumulated_study_seconds,
                   paused_stage_elapsed_seconds, status, ended_at, updated_at
            FROM study_modes
            WHERE id = ?1
            ",
            params![local_id],
            |row| {
                let phase_started_at: String = row.get(2)?;
                let paused_at: Option<String> = row.get(3)?;
                let ended_at: Option<String> = row.get(7)?;
                let updated_at: String = row.get(8)?;
                Ok(DbStudyModeProgress {
                    phase: row.get(0)?,
                    round_number: row.get(1)?,
                    phase_started_at: parse_rfc3339_millis(&phase_started_at).ok(),
                    paused_at: paused_at
                        .as_deref()
                        .and_then(|value| parse_rfc3339_millis(value).ok()),
                    accumulated_study_seconds: row.get(4)?,
                    paused_stage_elapsed_seconds: row.get(5)?,
                    status: row.get(6)?,
                    ended_at: ended_at
                        .as_deref()
                        .and_then(|value| parse_rfc3339_millis(value).ok()),
                    updated_at: parse_rfc3339_millis(&updated_at).unwrap_or_default(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn should_preserve_db_active_progress(
    existing: &DbStudyModeProgress,
    incoming: &DbStudyModeProgress,
    now_millis: i64,
) -> bool {
    if !is_shared_running_status(&existing.status) || !is_shared_running_status(&incoming.status) {
        return false;
    }
    let existing_mode = db_progress_to_shared_mode(existing);
    let incoming_mode = db_progress_to_shared_mode(incoming);
    active_mode_regresses(&existing_mode, &incoming_mode, now_millis)
        && incoming.updated_at >= existing.updated_at
}

fn db_progress_to_shared_mode(progress: &DbStudyModeProgress) -> SharedStudyMode {
    SharedStudyMode {
        sync_id: String::new(),
        state_revision: None,
        mode: None,
        subject_sync_id: None,
        planned_seconds: Some(i64::MAX / 4),
        focus_seconds: None,
        break_seconds: None,
        long_break_seconds: None,
        long_break_interval: None,
        phase: Some(progress.phase.clone()),
        round_number: Some(progress.round_number),
        started_at: None,
        phase_started_at: progress.phase_started_at,
        paused_at: progress.paused_at,
        paused_from_phase: None,
        accumulated_study_seconds: Some(progress.accumulated_study_seconds),
        total_paused_seconds: None,
        phase_paused_seconds: Some(progress.paused_stage_elapsed_seconds),
        paused_stage_elapsed_seconds: Some(progress.paused_stage_elapsed_seconds),
        current_break_type: None,
        ended_at: progress.ended_at,
        current_session_sync_id: None,
        schedule_block_sync_id: None,
        today_plan_item_sync_id: None,
        last_control_device_id: None,
        last_control_action: None,
        last_control_at: None,
        status: Some(progress.status.clone()),
        finish_reason: None,
        created_at: None,
        updated_at: progress.updated_at,
        deleted_at: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn upsert_focus_session_row(
    connection: &Connection,
    sync_id: &str,
    mode: &str,
    subject_id: Option<i64>,
    planned_seconds: i64,
    actual_seconds: i64,
    started_at: &str,
    ended_at: Option<String>,
    status: &str,
    end_reason: Option<String>,
    interruption_count: i64,
    emergency_exit_count: i64,
    paused_seconds: i64,
    followed_by_break_type: Option<String>,
    schedule_block_id: Option<i64>,
    today_plan_item_id: Option<i64>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_FOCUS_SESSION, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE focus_sessions
                SET mode = ?1,
                    subject_id = ?2,
                    planned_seconds = ?3,
                    actual_seconds = ?4,
                    started_at = ?5,
                    ended_at = ?6,
                    status = ?7,
                    end_reason = ?8,
                    interruption_count = ?9,
                    emergency_exit_count = ?10,
                    paused_seconds = ?11,
                    followed_by_break_type = ?12,
                    schedule_block_id = ?13,
                    today_plan_item_id = ?14,
                    created_at = ?15,
                    updated_at = ?16
                WHERE id = ?17
                ",
                params![
                    mode,
                    subject_id,
                    planned_seconds,
                    actual_seconds,
                    started_at,
                    ended_at,
                    status,
                    end_reason,
                    interruption_count,
                    emergency_exit_count,
                    paused_seconds,
                    followed_by_break_type,
                    schedule_block_id,
                    today_plan_item_id,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_FOCUS_SESSION,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO focus_sessions (
              mode, subject_id, planned_seconds, actual_seconds, started_at, ended_at,
              status, end_reason, interruption_count, emergency_exit_count,
              paused_seconds, followed_by_break_type, schedule_block_id, today_plan_item_id,
              created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ",
            params![
                mode,
                subject_id,
                planned_seconds,
                actual_seconds,
                started_at,
                ended_at,
                status,
                end_reason,
                interruption_count,
                emergency_exit_count,
                paused_seconds,
                followed_by_break_type,
                schedule_block_id,
                today_plan_item_id,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_FOCUS_SESSION,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_local_active_conflicts(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, updated_at, current_session_id
            FROM study_modes
            WHERE status = 'active'
            ORDER BY updated_at DESC, id DESC
            ",
        )
        .map_err(|error| error.to_string())?;
    let active_modes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    if active_modes.len() <= 1 {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    for (mode_id, _, current_session_id) in active_modes.into_iter().skip(1) {
        if let Some(session_id) = current_session_id {
            connection
                .execute(
                    "
                    UPDATE focus_sessions
                    SET status = 'finished',
                        ended_at = COALESCE(ended_at, ?1),
                        end_reason = COALESCE(end_reason, 'sync_takeover'),
                        updated_at = ?1
                    WHERE id = ?2 AND status = 'running'
                    ",
                    params![&now, session_id],
                )
                .map_err(|error| error.to_string())?;
        }

        connection
            .execute(
                "
                UPDATE study_modes
                SET status = 'finished',
                    phase = 'finished',
                    state_revision = state_revision + 1,
                    ended_at = COALESCE(ended_at, ?1),
                    current_session_id = NULL,
                    finish_reason = COALESCE(finish_reason, 'sync_takeover'),
                    updated_at = ?1
                WHERE id = ?2 AND status = 'active'
                ",
                params![&now, mode_id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_app_event_row(
    connection: &Connection,
    sync_id: &str,
    focus_session_id: Option<i64>,
    package_name: &str,
    app_name: Option<String>,
    event_type: &str,
    action: Option<String>,
    created_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_APP_EVENT, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE app_events
                SET session_id = COALESCE(?1, session_id),
                    process_name = ?2,
                    process_path = NULL,
                    window_title = ?3,
                    event_type = ?4,
                    action_taken = ?5,
                    created_at = ?6
                WHERE id = ?7
                ",
                params![
                    focus_session_id,
                    package_name,
                    app_name,
                    event_type,
                    action,
                    created_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_APP_EVENT,
            local_id,
            sync_id,
            parse_rfc3339_millis(created_at)?,
            None,
        )?;
        return Ok(());
    }

    let Some(session_id) = focus_session_id else {
        return Ok(());
    };

    connection
        .execute(
            "
            INSERT INTO app_events (
              session_id, process_name, process_path, window_title, event_type, action_taken, created_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)
            ",
            params![session_id, package_name, app_name, event_type, action, created_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_APP_EVENT,
        local_id,
        sync_id,
        parse_rfc3339_millis(created_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_checklist_task_row(
    connection: &Connection,
    sync_id: &str,
    board_scope: &str,
    subject_id: Option<i64>,
    column_id: i64,
    title: &str,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_CHECKLIST_TASK, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE checklist_tasks
                SET board_scope = ?1,
                    subject_id = ?2,
                    column_id = ?3,
                    title = ?4,
                    note = ?5,
                    due_date = ?6,
                    sort_order = ?7,
                    completed = ?8,
                    created_at = ?9,
                    updated_at = ?10
                WHERE id = ?11
                ",
                params![
                    board_scope,
                    subject_id,
                    column_id,
                    title,
                    note,
                    due_date,
                    sort_order,
                    if completed { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_CHECKLIST_TASK,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO checklist_tasks (
              board_scope, subject_id, column_id, title, note, due_date, sort_order, completed, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![board_scope, subject_id, column_id, title, note, due_date, sort_order, if completed { 1 } else { 0 }, created_at, updated_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_CHECKLIST_TASK,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_today_plan_item_row(
    connection: &Connection,
    sync_id: &str,
    today_date: &str,
    source_task_id: Option<i64>,
    subject_id: Option<i64>,
    title: &str,
    note: Option<String>,
    due_date: Option<String>,
    sort_order: i64,
    completed: bool,
    synced_source_completion: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_TODAY_PLAN_ITEM, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE today_plan_items
                SET today_date = ?1,
                    source_task_id = ?2,
                    subject_id = ?3,
                    title = ?4,
                    note = ?5,
                    due_date = ?6,
                    sort_order = ?7,
                    completed = ?8,
                    synced_source_completion = ?9,
                    created_at = ?10,
                    updated_at = ?11
                WHERE id = ?12
                ",
                params![
                    today_date,
                    source_task_id,
                    subject_id,
                    title,
                    note,
                    due_date,
                    sort_order,
                    if completed { 1 } else { 0 },
                    if synced_source_completion { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_TODAY_PLAN_ITEM,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO today_plan_items (
              today_date, source_task_id, subject_id, title, note, due_date, sort_order, completed, synced_source_completion, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            params![today_date, source_task_id, subject_id, title, note, due_date, sort_order, if completed { 1 } else { 0 }, if synced_source_completion { 1 } else { 0 }, created_at, updated_at],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_TODAY_PLAN_ITEM,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_schedule_template_row(
    connection: &Connection,
    sync_id: &str,
    title: &str,
    note: Option<String>,
    category_key: &str,
    subject_id: Option<i64>,
    weekdays: &str,
    start_minute: i64,
    end_minute: i64,
    enabled: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_local_id_by_sync_id(connection, ENTITY_SCHEDULE_TEMPLATE, Some(sync_id))?
    {
        connection
            .execute(
                "
                UPDATE schedule_templates
                SET title = ?1,
                    note = ?2,
                    category_key = ?3,
                    subject_id = ?4,
                    weekdays = ?5,
                    start_minute = ?6,
                    end_minute = ?7,
                    enabled = ?8,
                    created_at = ?9,
                    updated_at = ?10
                WHERE id = ?11
                ",
                params![
                    title,
                    note,
                    category_key,
                    subject_id,
                    weekdays,
                    start_minute,
                    end_minute,
                    if enabled { 1 } else { 0 },
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SCHEDULE_TEMPLATE,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO schedule_templates (
              title, note, category_key, subject_id, weekdays, start_minute, end_minute,
              enabled, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                title,
                note,
                category_key,
                subject_id,
                weekdays,
                start_minute,
                end_minute,
                if enabled { 1 } else { 0 },
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SCHEDULE_TEMPLATE,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_schedule_block_row(
    connection: &Connection,
    sync_id: &str,
    schedule_date: &str,
    title: &str,
    note: Option<String>,
    category_key: &str,
    subject_id: Option<i64>,
    source_today_item_id: Option<i64>,
    template_id: Option<i64>,
    start_minute: i64,
    end_minute: i64,
    status: &str,
    linked_study_mode_id: Option<i64>,
    linked_focus_session_id: Option<i64>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    if let Some(local_id) =
        resolve_schedule_block_import_id(connection, sync_id, schedule_date, template_id)?
    {
        connection
            .execute(
                "
                UPDATE schedule_blocks
                SET schedule_date = ?1,
                    title = ?2,
                    note = ?3,
                    category_key = ?4,
                    subject_id = ?5,
                    source_today_item_id = ?6,
                    template_id = ?7,
                    start_minute = ?8,
                    end_minute = ?9,
                    status = ?10,
                    linked_study_mode_id = ?11,
                    linked_focus_session_id = ?12,
                    created_at = ?13,
                    updated_at = ?14
                WHERE id = ?15
                ",
                params![
                    schedule_date,
                    title,
                    note,
                    category_key,
                    subject_id,
                    source_today_item_id,
                    template_id,
                    start_minute,
                    end_minute,
                    status,
                    linked_study_mode_id,
                    linked_focus_session_id,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_SCHEDULE_BLOCK,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO schedule_blocks (
              schedule_date, title, note, category_key, subject_id, source_today_item_id,
              template_id, start_minute, end_minute, status, linked_study_mode_id,
              linked_focus_session_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ",
            params![
                schedule_date,
                title,
                note,
                category_key,
                subject_id,
                source_today_item_id,
                template_id,
                start_minute,
                end_minute,
                status,
                linked_study_mode_id,
                linked_focus_session_id,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_schedule_block_import_id(
    connection: &Connection,
    sync_id: &str,
    schedule_date: &str,
    template_id: Option<i64>,
) -> Result<Option<i64>, String> {
    let by_template_date = match template_id {
        Some(template_id) => connection
            .query_row(
                "
                SELECT id
                FROM schedule_blocks
                WHERE template_id = ?1 AND schedule_date = ?2
                LIMIT 1
                ",
                params![template_id, schedule_date],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?,
        None => None,
    };

    Ok(by_template_date.or(resolve_local_id_by_sync_id(
        connection,
        ENTITY_SCHEDULE_BLOCK,
        Some(sync_id),
    )?))
}

#[allow(clippy::too_many_arguments)]
fn upsert_daily_review_row(
    connection: &Connection,
    sync_id: &str,
    review_date: &str,
    summary: Option<String>,
    blockers: Option<String>,
    tomorrow_focus: Option<String>,
    mood_score: i64,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let existing_id = resolve_local_id_by_sync_id(connection, ENTITY_DAILY_REVIEW, Some(sync_id))?
        .or_else(|| {
            connection
                .query_row(
                    "SELECT id FROM daily_reviews WHERE review_date = ?1",
                    params![review_date],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .ok()
                .flatten()
        });

    if let Some(local_id) = existing_id {
        connection
            .execute(
                "
                UPDATE daily_reviews
                SET review_date = ?1,
                    summary = ?2,
                    blockers = ?3,
                    tomorrow_focus = ?4,
                    mood_score = ?5,
                    created_at = ?6,
                    updated_at = ?7
                WHERE id = ?8
                ",
                params![
                    review_date,
                    summary,
                    blockers,
                    tomorrow_focus,
                    mood_score,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_DAILY_REVIEW,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO daily_reviews (
              review_date, summary, blockers, tomorrow_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                review_date,
                summary,
                blockers,
                tomorrow_focus,
                mood_score,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_DAILY_REVIEW,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_weekly_review_row(
    connection: &Connection,
    sync_id: &str,
    week_start_date: &str,
    summary: Option<String>,
    blockers: Option<String>,
    next_week_focus: Option<String>,
    mood_score: i64,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let existing_id = resolve_local_id_by_sync_id(connection, ENTITY_WEEKLY_REVIEW, Some(sync_id))?
        .or_else(|| {
            connection
                .query_row(
                    "SELECT id FROM weekly_reviews WHERE week_start_date = ?1",
                    params![week_start_date],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .ok()
                .flatten()
        });

    if let Some(local_id) = existing_id {
        connection
            .execute(
                "
                UPDATE weekly_reviews
                SET week_start_date = ?1,
                    summary = ?2,
                    blockers = ?3,
                    next_week_focus = ?4,
                    mood_score = ?5,
                    created_at = ?6,
                    updated_at = ?7
                WHERE id = ?8
                ",
                params![
                    week_start_date,
                    summary,
                    blockers,
                    next_week_focus,
                    mood_score,
                    created_at,
                    updated_at,
                    local_id
                ],
            )
            .map_err(|error| error.to_string())?;
        upsert_sync_meta(
            connection,
            ENTITY_WEEKLY_REVIEW,
            local_id,
            sync_id,
            parse_rfc3339_millis(updated_at)?,
            None,
        )?;
        return Ok(());
    }

    connection
        .execute(
            "
            INSERT INTO weekly_reviews (
              week_start_date, summary, blockers, next_week_focus, mood_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                week_start_date,
                summary,
                blockers,
                next_week_focus,
                mood_score,
                created_at,
                updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
    let local_id = connection.last_insert_rowid();
    upsert_sync_meta(
        connection,
        ENTITY_WEEKLY_REVIEW,
        local_id,
        sync_id,
        parse_rfc3339_millis(updated_at)?,
        None,
    )
}

fn resolve_or_create_sync_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    preferred_sync_id: Option<String>,
    updated_at: i64,
) -> Result<String, String> {
    if let Some(meta) = get_sync_meta_by_local_id(connection, entity_type, local_id)? {
        upsert_sync_meta(
            connection,
            entity_type,
            local_id,
            &meta.sync_id,
            updated_at,
            None,
        )?;
        return Ok(meta.sync_id);
    }

    let fallback_sync_id = format!("{entity_type}-{local_id}");
    let sync_id = match preferred_sync_id {
        Some(preferred) => match get_sync_meta_by_sync_id(connection, &preferred)? {
            Some(existing) if existing.local_id != local_id => fallback_sync_id,
            _ => preferred,
        },
        None => fallback_sync_id,
    };
    upsert_sync_meta(
        connection,
        entity_type,
        local_id,
        &sync_id,
        updated_at,
        None,
    )?;
    Ok(sync_id)
}

fn upsert_sync_meta(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    sync_id: &str,
    updated_at: i64,
    deleted_at: Option<i64>,
) -> Result<(), String> {
    connection
        .execute(
            "
            DELETE FROM sync_meta
            WHERE sync_id = ?1
              AND NOT (entity_type = ?2 AND local_id = ?3)
            ",
            params![sync_id, entity_type, local_id],
        )
        .map_err(|error| error.to_string())?;

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
            params![entity_type, local_id, sync_id, deleted_at, updated_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_sync_meta_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
) -> Result<Option<SyncMetaRow>, String> {
    connection
        .query_row(
            "
            SELECT local_id, sync_id, deleted_at
            FROM sync_meta
            WHERE entity_type = ?1 AND local_id = ?2
            ",
            params![entity_type, local_id],
            |row| {
                Ok(SyncMetaRow {
                    local_id: row.get(0)?,
                    sync_id: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_sync_meta_by_sync_id(
    connection: &Connection,
    sync_id: &str,
) -> Result<Option<SyncMetaRow>, String> {
    connection
        .query_row(
            "
            SELECT local_id, sync_id, deleted_at
            FROM sync_meta
            WHERE sync_id = ?1
            ",
            params![sync_id],
            |row| {
                Ok(SyncMetaRow {
                    local_id: row.get(0)?,
                    sync_id: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn delete_local_row_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: &str,
    deleted_at: i64,
) -> Result<(), String> {
    let local_id = resolve_local_id_by_sync_id(connection, entity_type, Some(sync_id))?;
    if let Some(local_id) = local_id {
        match entity_type {
            ENTITY_SUBJECT => {
                connection
                    .execute("DELETE FROM subjects WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_STUDY_MODE => {
                connection
                    .execute(
                        "
                        UPDATE schedule_blocks
                        SET linked_study_mode_id = NULL
                        WHERE linked_study_mode_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute("DELETE FROM study_modes WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_FOCUS_SESSION => {
                connection
                    .execute(
                        "
                        UPDATE study_modes
                        SET current_session_id = NULL
                        WHERE current_session_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "
                        UPDATE schedule_blocks
                        SET linked_focus_session_id = NULL
                        WHERE linked_focus_session_id = ?1
                        ",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM app_events WHERE session_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM focus_sessions WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_APP_EVENT => {
                connection
                    .execute("DELETE FROM app_events WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_CHECKLIST_TASK => {
                connection
                    .execute(
                        "DELETE FROM today_plan_items WHERE source_task_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM checklist_tasks WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_TODAY_PLAN_ITEM => {
                connection
                    .execute(
                        "DELETE FROM today_plan_items WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_SCHEDULE_BLOCK => {
                connection
                    .execute(
                        "DELETE FROM schedule_blocks WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_SCHEDULE_TEMPLATE => {
                connection
                    .execute(
                        "DELETE FROM schedule_blocks WHERE template_id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "DELETE FROM schedule_templates WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_DAILY_REVIEW => {
                connection
                    .execute("DELETE FROM daily_reviews WHERE id = ?1", params![local_id])
                    .map_err(|error| error.to_string())?;
            }
            ENTITY_WEEKLY_REVIEW => {
                connection
                    .execute(
                        "DELETE FROM weekly_reviews WHERE id = ?1",
                        params![local_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
            _ => {}
        }
    }

    let tombstone_local_id =
        local_id.unwrap_or_else(|| synthetic_local_id_for_sync_id(entity_type, sync_id));
    upsert_sync_meta(
        connection,
        entity_type,
        tombstone_local_id,
        sync_id,
        deleted_at,
        Some(deleted_at),
    )?;
    Ok(())
}

fn resolve_local_id_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: Option<&str>,
) -> Result<Option<i64>, String> {
    let Some(sync_id) = sync_id else {
        return Ok(None);
    };
    let sync_id = if entity_type == ENTITY_SUBJECT {
        canonical_subject_sync_id(sync_id)
    } else {
        sync_id.to_string()
    };

    let meta = get_sync_meta_by_sync_id(connection, &sync_id)?;
    if let Some(meta) = meta {
        if meta.deleted_at.is_some() {
            return Ok(None);
        }
        return Ok(Some(meta.local_id));
    }

    let _ = entity_type;
    Ok(None)
}

fn resolve_existing_local_id_by_sync_id(
    connection: &Connection,
    entity_type: &str,
    sync_id: Option<&str>,
) -> Result<Option<i64>, String> {
    let Some(local_id) = resolve_local_id_by_sync_id(connection, entity_type, sync_id)? else {
        return Ok(None);
    };

    if local_row_exists(connection, entity_type, local_id)? {
        Ok(Some(local_id))
    } else {
        Ok(None)
    }
}

fn resolve_sync_id_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(local_id) = local_id else {
        return Ok(None);
    };

    let sync_id = connection
        .query_row(
            "
            SELECT sync_id
            FROM sync_meta
            WHERE entity_type = ?1 AND local_id = ?2
            ",
            params![entity_type, local_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(if entity_type == ENTITY_SUBJECT {
        sync_id.map(|value| canonical_subject_sync_id(&value))
    } else {
        sync_id
    })
}

fn resolve_existing_sync_id_by_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: Option<i64>,
) -> Result<Option<String>, String> {
    let Some(local_id) = local_id else {
        return Ok(None);
    };
    if !local_row_exists(connection, entity_type, local_id)? {
        return Ok(None);
    }
    resolve_sync_id_by_local_id(connection, entity_type, Some(local_id))
}

fn local_row_exists(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
) -> Result<bool, String> {
    let table = match entity_type {
        ENTITY_SUBJECT => "subjects",
        ENTITY_STUDY_MODE => "study_modes",
        ENTITY_FOCUS_SESSION => "focus_sessions",
        ENTITY_APP_EVENT => "app_events",
        ENTITY_CHECKLIST_TASK => "checklist_tasks",
        ENTITY_TODAY_PLAN_ITEM => "today_plan_items",
        ENTITY_SCHEDULE_BLOCK => "schedule_blocks",
        ENTITY_SCHEDULE_TEMPLATE => "schedule_templates",
        ENTITY_DAILY_REVIEW => "daily_reviews",
        ENTITY_WEEKLY_REVIEW => "weekly_reviews",
        _ => return Ok(false),
    };
    let exists: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1"),
            params![local_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(exists > 0)
}

fn resolve_study_mode_sync_id_for_session(
    connection: &Connection,
    session_id: i64,
) -> Result<Option<String>, String> {
    let study_mode_id = connection
        .query_row(
            "
            SELECT id
            FROM study_modes
            WHERE current_session_id = ?1
            ORDER BY updated_at DESC, id DESC
            LIMIT 1
            ",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    resolve_existing_sync_id_by_local_id(connection, ENTITY_STUDY_MODE, study_mode_id)
}

fn synthetic_local_id_for_sync_id(entity_type: &str, sync_id: &str) -> i64 {
    let mut hasher = DefaultHasher::new();
    entity_type.hash(&mut hasher);
    sync_id.hash(&mut hasher);
    let hash = hasher.finish() as i64;
    let positive = hash.unsigned_abs() as i64;
    if positive == 0 {
        1
    } else {
        positive
    }
}

fn board_scope_for_category_key(category_key: &str) -> String {
    match category_key {
        "politics" => "checklist:politics".to_string(),
        "english" => "checklist:english".to_string(),
        "math" => "checklist:math".to_string(),
        "major" => "checklist:major".to_string(),
        _ => "checklist:general".to_string(),
    }
}

fn map_board_scope_to_category_key(board_scope: &str) -> String {
    match board_scope {
        "checklist:politics" => "politics".to_string(),
        "checklist:english" => "english".to_string(),
        "checklist:math" => "math".to_string(),
        "checklist:major" => "major".to_string(),
        _ => "general".to_string(),
    }
}

fn parse_weekdays_json(raw: &str) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|weekday| matches!(*weekday, 1..=7))
        .collect()
}

fn default_subject_sync_id(name: &str, local_id: i64) -> String {
    canonical_subject_sync_id_for_name(name)
        .map(str::to_string)
        .unwrap_or_else(|| format!("subject-{local_id}"))
}

fn normalize_name(value: &str) -> String {
    value.trim().to_string()
}

fn parse_rfc3339_millis(value: &str) -> Result<i64, String> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|error| error.to_string())?
        .with_timezone(&Utc)
        .timestamp_millis())
}

fn millis_to_rfc3339(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn ensure_device_id(connection: &Connection) -> Result<String, String> {
    let existing = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'sync_device_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(device_id) = existing {
        if !device_id.trim().is_empty() {
            return Ok(device_id);
        }
    }

    let device_id = Uuid::new_v4().to_string();
    let now = millis_to_rfc3339(Utc::now().timestamp_millis());
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES ('sync_device_id', ?1, ?2)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![device_id, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(device_id)
}

pub fn load_or_create_device_id(connection: &Connection) -> Result<String, String> {
    ensure_device_id(connection)
}

pub fn ensure_sync_meta_for_local_id(
    connection: &Connection,
    entity_type: &str,
    local_id: i64,
    preferred_sync_id: Option<String>,
    updated_at: i64,
) -> Result<String, String> {
    resolve_or_create_sync_id(
        connection,
        entity_type,
        local_id,
        preferred_sync_id,
        updated_at,
    )
}

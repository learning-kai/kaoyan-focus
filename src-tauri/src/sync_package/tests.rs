
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_database;
    use tempfile::tempdir;
    fn empty_payload(device_id: &str, exported_at: i64) -> SharedSyncPayload {
        SharedSyncPayload {
            schema_version: SYNC_SCHEMA_VERSION,
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

    fn running_study_mode(
        sync_id: &str,
        round_number: i64,
        phase: &str,
        accumulated_study_seconds: i64,
        phase_started_at: i64,
        updated_at: i64,
    ) -> SharedStudyMode {
        SharedStudyMode {
            sync_id: sync_id.to_string(),
            state_revision: Some(round_number.max(1)),
            mode: Some("normal".to_string()),
            subject_sync_id: None,
            planned_seconds: Some(3600),
            focus_seconds: Some(1500),
            break_seconds: Some(300),
            long_break_seconds: Some(900),
            long_break_interval: Some(4),
            phase: Some(phase.to_string()),
            round_number: Some(round_number),
            started_at: Some(0),
            phase_started_at: Some(phase_started_at),
            paused_at: None,
            paused_from_phase: None,
            accumulated_study_seconds: Some(accumulated_study_seconds),
            total_paused_seconds: Some(0),
            phase_paused_seconds: Some(0),
            paused_stage_elapsed_seconds: Some(0),
            current_break_type: Some("short".to_string()),
            ended_at: None,
            current_session_sync_id: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            last_control_device_id: None,
            last_control_action: None,
            last_control_at: None,
            status: Some("running".to_string()),
            finish_reason: None,
            created_at: Some(0),
            updated_at,
            deleted_at: None,
        }
    }

    fn focus_session(
        sync_id: &str,
        study_mode_sync_id: &str,
        status: &str,
        updated_at: i64,
    ) -> SharedFocusSession {
        SharedFocusSession {
            sync_id: sync_id.to_string(),
            study_mode_sync_id: Some(study_mode_sync_id.to_string()),
            subject_sync_id: None,
            mode: Some("normal".to_string()),
            planned_seconds: Some(1500),
            actual_seconds: Some(0),
            started_at: Some(0),
            ended_at: None,
            status: Some(status.to_string()),
            end_reason: None,
            interruption_count: Some(0),
            emergency_exit_count: Some(0),
            paused_seconds: Some(0),
            followed_by_break_type: None,
            schedule_block_sync_id: None,
            today_plan_item_sync_id: None,
            created_at: Some(0),
            updated_at,
            deleted_at: None,
        }
    }

    fn with_control(
        mut mode: SharedStudyMode,
        device_id: &str,
        action: &str,
        at: i64,
    ) -> SharedStudyMode {
        mode.last_control_device_id = Some(device_id.to_string());
        mode.last_control_action = Some(action.to_string());
        mode.last_control_at = Some(at);
        mode.updated_at = at;
        mode
    }

    #[test]
    fn checklist_tombstone_wins_over_same_timestamp_live_item() {
        let mut local = empty_payload("desktop", 1000);
        local.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-1".to_string(),
            category_key: Some("math".to_string()),
            subject_sync_id: None,
            title: Some("old".to_string()),
            note: None,
            due_date: None,
            sort_order: Some(0.0),
            completed: Some(false),
            created_at: Some(900),
            updated_at: 2000,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-1".to_string(),
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: 2000,
            deleted_at: Some(2000),
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.checklist_tasks.len(), 1);
        assert_eq!(merged.checklist_tasks[0].deleted_at, Some(2000));
    }

    #[test]
    fn newer_tombstone_wins_over_older_schedule_block() {
        let mut local = empty_payload("desktop", 1000);
        local.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "block-1".to_string(),
            schedule_date: Some("2026-05-18".to_string()),
            title: Some("math".to_string()),
            note: None,
            category_key: Some("math".to_string()),
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: None,
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 1500,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "block-1".to_string(),
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
            updated_at: 2500,
            deleted_at: Some(2500),
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.schedule_blocks.len(), 1);
        assert_eq!(merged.schedule_blocks[0].deleted_at, Some(2500));
    }

    #[test]
    fn active_study_mode_never_rolls_back_to_older_round_even_with_newer_timestamp() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "mode-1", 2, "focus", 1500, 50_000, 2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 0, 10_000, 3_000));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].round_number, Some(2));
        assert_eq!(merged.study_modes[0].accumulated_study_seconds, Some(1500));
    }

    #[test]
    fn active_study_mode_accepts_remote_pause_command() {
        let mut local = empty_payload("desktop", 60_000);
        local
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 100, 50_000, 2_000));
        let mut remote = empty_payload("phone", 60_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 100, 50_000, 3_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        paused.paused_stage_elapsed_seconds = Some(5);
        remote.study_modes.push(paused);

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("paused"));
        assert_eq!(merged.study_modes[0].paused_at, Some(55_000));
    }

    #[test]
    fn primary_local_active_rejects_remote_pause_without_control_intent() {
        let mut local = empty_payload("desktop", 60_000);
        local.primary_owner_device_id = Some("desktop".to_string());
        local.primary_owner_updated_at = Some(10_000);
        local
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 900, 50_000, 2_000));

        let mut remote = empty_payload("phone", 60_000);
        remote.primary_owner_device_id = Some("desktop".to_string());
        remote.primary_owner_updated_at = Some(10_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 900, 50_000, 6_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        paused.paused_stage_elapsed_seconds = Some(5);
        remote.study_modes.push(paused);

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("focus"));
        assert_eq!(merged.study_modes[0].paused_at, None);
    }

    #[test]
    fn remote_pause_control_does_not_reduce_accepted_progress() {
        let mut local = empty_payload("desktop", 60_000);
        local.primary_owner_device_id = Some("desktop".to_string());
        local.primary_owner_updated_at = Some(10_000);
        local.study_modes.push(running_study_mode(
            "mode-1", 2, "focus", 1_600, 50_000, 2_000,
        ));

        let mut remote = empty_payload("phone", 60_000);
        remote.primary_owner_device_id = Some("desktop".to_string());
        remote.primary_owner_updated_at = Some(10_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 100, 50_000, 6_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        paused.paused_stage_elapsed_seconds = Some(5);
        remote
            .study_modes
            .push(with_control(paused, "phone", "pause", 6_000));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("paused"));
        assert_eq!(merged.study_modes[0].round_number, Some(2));
        assert_eq!(merged.study_modes[0].accumulated_study_seconds, Some(1_600));
    }

    #[test]
    fn active_study_mode_accepts_remote_resume_command() {
        let mut local = empty_payload("desktop", 60_000);
        let mut paused = running_study_mode("mode-1", 1, "paused", 100, 50_000, 2_000);
        paused.paused_at = Some(55_000);
        paused.paused_from_phase = Some("focus".to_string());
        local.study_modes.push(paused);
        let mut remote = empty_payload("phone", 60_000);
        remote
            .study_modes
            .push(running_study_mode("mode-1", 1, "focus", 100, 56_000, 3_000));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("focus"));
        assert_eq!(merged.study_modes[0].paused_at, None);
    }

    #[test]
    fn active_study_mode_accepts_remote_start_break_command() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "mode-1",
            1,
            "awaiting_break",
            1500,
            50_000,
            2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.study_modes.push(running_study_mode(
            "mode-1", 1, "break", 1500, 55_000, 3_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("break"));
    }

    #[test]
    fn active_study_mode_accepts_remote_finish_command_and_session() {
        let mut local = empty_payload("desktop", 60_000);
        let mut local_mode = running_study_mode("mode-1", 1, "focus", 100, 50_000, 2_000);
        local_mode.current_session_sync_id = Some("session-1".to_string());
        local.study_modes.push(local_mode);
        local
            .focus_sessions
            .push(focus_session("session-1", "mode-1", "running", 2_000));

        let mut remote = empty_payload("phone", 60_000);
        let mut finished = running_study_mode("mode-1", 1, "finished", 120, 50_000, 3_000);
        finished.status = Some("finished".to_string());
        finished.ended_at = Some(58_000);
        finished.current_session_sync_id = Some("session-1".to_string());
        remote.study_modes.push(finished);
        let mut session = focus_session("session-1", "mode-1", "finished", 3_000);
        session.actual_seconds = Some(120);
        session.ended_at = Some(58_000);
        remote.focus_sessions.push(session);

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        assert_eq!(merged.study_modes.len(), 1);
        assert_eq!(merged.study_modes[0].status.as_deref(), Some("finished"));
        assert_eq!(merged.study_modes[0].phase.as_deref(), Some("finished"));
        assert_eq!(merged.focus_sessions.len(), 1);
        assert_eq!(merged.focus_sessions[0].status.as_deref(), Some("finished"));
        assert_eq!(merged.focus_sessions[0].actual_seconds, Some(120));
    }

    #[test]
    fn different_remote_active_does_not_take_over_local_active() {
        let mut local = empty_payload("desktop", 60_000);
        local.study_modes.push(running_study_mode(
            "desktop-mode",
            1,
            "focus",
            300,
            50_000,
            2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.study_modes.push(running_study_mode(
            "phone-mode",
            1,
            "focus",
            0,
            59_000,
            3_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        let desktop_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "desktop-mode")
            .expect("desktop active should remain");
        let phone_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "phone-mode")
            .expect("phone mode should be retained as history");
        assert_eq!(desktop_mode.status.as_deref(), Some("running"));
        assert_eq!(desktop_mode.phase.as_deref(), Some("focus"));
        assert_eq!(phone_mode.status.as_deref(), Some("finished"));
        assert_eq!(phone_mode.finish_reason.as_deref(), Some("sync_takeover"));
    }

    #[test]
    fn primary_local_active_rejects_different_remote_takeover() {
        let mut local = empty_payload("desktop", 60_000);
        local.primary_owner_device_id = Some("desktop".to_string());
        local.study_modes.push(running_study_mode(
            "desktop-mode",
            1,
            "focus",
            300,
            50_000,
            2_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.primary_owner_device_id = Some("desktop".to_string());
        remote.study_modes.push(running_study_mode(
            "phone-mode",
            3,
            "focus",
            0,
            59_000,
            5_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        let desktop_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "desktop-mode")
            .expect("desktop active should remain");
        let phone_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "phone-mode")
            .expect("phone mode should be retained as history");
        assert_eq!(merged.primary_owner_device_id.as_deref(), Some("desktop"));
        assert_eq!(desktop_mode.status.as_deref(), Some("running"));
        assert_eq!(phone_mode.status.as_deref(), Some("finished"));
        assert_eq!(phone_mode.finish_reason.as_deref(), Some("sync_takeover"));
    }

    #[test]
    fn remote_primary_active_takes_over_local_non_primary_active() {
        let mut local = empty_payload("desktop", 60_000);
        local.primary_owner_device_id = Some("phone".to_string());
        local.study_modes.push(running_study_mode(
            "desktop-mode",
            3,
            "focus",
            900,
            50_000,
            5_000,
        ));
        let mut remote = empty_payload("phone", 60_000);
        remote.primary_owner_device_id = Some("phone".to_string());
        remote.study_modes.push(running_study_mode(
            "phone-mode",
            1,
            "focus",
            120,
            59_000,
            3_000,
        ));

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 60_000);
        let desktop_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "desktop-mode")
            .expect("desktop mode should be retained as history");
        let phone_mode = merged
            .study_modes
            .iter()
            .find(|mode| mode.sync_id == "phone-mode")
            .expect("phone active should win");
        assert_eq!(merged.primary_owner_device_id.as_deref(), Some("phone"));
        assert_eq!(phone_mode.status.as_deref(), Some("running"));
        assert_eq!(desktop_mode.status.as_deref(), Some("finished"));
        assert_eq!(desktop_mode.finish_reason.as_deref(), Some("sync_takeover"));
    }

    #[test]
    fn schedule_blocks_with_same_stable_sync_id_do_not_duplicate() {
        let mut local = empty_payload("desktop", 1000);
        local.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "schedule_block:2026-05-20:template:7:480-540".to_string(),
            schedule_date: Some("2026-05-20".to_string()),
            title: Some("math".to_string()),
            note: None,
            category_key: Some("math".to_string()),
            subject_sync_id: Some("subject-3".to_string()),
            source_today_item_sync_id: None,
            template_sync_id: Some("schedule_template-7".to_string()),
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 1000,
            deleted_at: None,
        });
        let mut remote = empty_payload("phone", 2000);
        remote.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "schedule_block:2026-05-20:template:7:480-540".to_string(),
            schedule_date: Some("2026-05-20".to_string()),
            title: Some("math".to_string()),
            note: Some("updated".to_string()),
            category_key: Some("math".to_string()),
            subject_sync_id: Some("subject-3".to_string()),
            source_today_item_sync_id: None,
            template_sync_id: Some("schedule_template-7".to_string()),
            start_minute: Some(480),
            end_minute: Some(540),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(1000),
            updated_at: 2000,
            deleted_at: None,
        });

        let merged = merge_shared_sync_payloads(local, remote, "desktop".to_string(), 3000);
        assert_eq!(merged.schedule_blocks.len(), 1);
        assert_eq!(merged.schedule_blocks[0].note.as_deref(), Some("updated"));
    }

    #[test]
    fn importing_remote_tombstone_preserves_remote_deleted_at() {
        let directory = tempdir().expect("create temp directory");
        let mut connection =
            open_database(&directory.path().join("sync-test.sqlite3")).expect("open test db");
        let mut payload = empty_payload("phone", 5_000);
        payload.checklist_tasks.push(SharedChecklistTask {
            sync_id: "task-deleted-remotely".to_string(),
            category_key: None,
            subject_sync_id: None,
            title: None,
            note: None,
            due_date: None,
            sort_order: None,
            completed: None,
            created_at: None,
            updated_at: 2_000,
            deleted_at: Some(2_000),
        });

        import_shared_sync_payload(&mut connection, &payload).expect("import payload");

        let (deleted_at, updated_at): (i64, i64) = connection
            .query_row(
                "SELECT deleted_at, updated_at FROM sync_meta WHERE sync_id = ?1",
                params!["task-deleted-remotely"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read tombstone meta");
        assert_eq!(deleted_at, 2_000);
        assert_eq!(updated_at, 2_000);
    }

    #[test]
    fn importing_schedule_block_reuses_existing_template_date_row() {
        let directory = tempdir().expect("create temp directory");
        let mut connection =
            open_database(&directory.path().join("sync-test.sqlite3")).expect("open test db");
        upsert_schedule_template_row(
            &connection,
            "template-1",
            "template",
            None,
            "general",
            None,
            "[1]",
            480,
            540,
            true,
            &millis_to_rfc3339(1_000),
            &millis_to_rfc3339(1_000),
        )
        .expect("insert template");
        let template_id: i64 = connection
            .query_row(
                "SELECT id FROM schedule_templates WHERE title = ?1",
                params!["template"],
                |row| row.get(0),
            )
            .expect("read template id");
        upsert_schedule_block_row(
            &connection,
            "local-block",
            "2030-02-01",
            "old block",
            None,
            "general",
            None,
            None,
            Some(template_id),
            480,
            540,
            "planned",
            None,
            None,
            &millis_to_rfc3339(1_000),
            &millis_to_rfc3339(1_000),
        )
        .expect("insert local generated block");

        let mut payload = empty_payload("phone", 2_000);
        payload.schedule_blocks.push(SharedScheduleBlock {
            sync_id: "remote-block".to_string(),
            schedule_date: Some("2030-02-01".to_string()),
            title: Some("remote block".to_string()),
            note: Some("updated".to_string()),
            category_key: Some("general".to_string()),
            subject_sync_id: None,
            source_today_item_sync_id: None,
            template_sync_id: Some("template-1".to_string()),
            start_minute: Some(500),
            end_minute: Some(560),
            status: Some("planned".to_string()),
            linked_study_mode_sync_id: None,
            linked_focus_session_sync_id: None,
            created_at: Some(2_000),
            updated_at: 2_000,
            deleted_at: None,
        });

        import_shared_sync_payload(&mut connection, &payload).expect("import payload");

        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schedule_blocks WHERE template_id = ?1 AND schedule_date = ?2",
                params![template_id, "2030-02-01"],
                |row| row.get(0),
            )
            .expect("count blocks");
        let (title, sync_id): (String, String) = connection
            .query_row(
                "
                SELECT b.title, m.sync_id
                FROM schedule_blocks b
                JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
                WHERE b.template_id = ?1 AND b.schedule_date = ?2
                ",
                params![template_id, "2030-02-01"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read merged block");

        assert_eq!(count, 1);
        assert_eq!(title, "remote block");
        assert_eq!(sync_id, "remote-block");
    }
}

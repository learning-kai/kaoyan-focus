#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod live_tests {
    use super::*;

    #[test]
    fn recognizes_deleted_calendar_event_error() {
        assert!(is_feishu_deleted_event_error(
            "Feishu returned status 403 / code 193003: event is deleted"
        ));
        assert!(!is_feishu_deleted_event_error(
            "Feishu returned status 403 / code 193004: permission denied"
        ));
    }

    #[test]
    fn skips_untitled_remote_calendar_imports() {
        let event = parse_remote_event(&json!({
            "event_id": "remote-blank-title",
            "description": "",
            "start_time": { "timestamp": "1779235200" },
            "end_time": { "timestamp": "1779238800" },
            "updated_time": "1779235200000"
        }))
        .expect("event parses");

        assert_eq!(event.title, "");
        assert!(!is_importable_remote_event(&event));
    }

    #[test]
    fn finds_orphan_calendar_event_links() {
        let connection = Connection::open_in_memory().expect("in-memory db");
        connection
            .execute_batch(
                "
                CREATE TABLE schedule_blocks (
                  id INTEGER PRIMARY KEY,
                  title TEXT NOT NULL
                );
                CREATE TABLE sync_meta (
                  entity_type TEXT NOT NULL,
                  local_id INTEGER NOT NULL,
                  sync_id TEXT NOT NULL,
                  deleted_at INTEGER,
                  updated_at INTEGER,
                  PRIMARY KEY (entity_type, local_id)
                );
                CREATE TABLE feishu_sync_links (
                  id INTEGER PRIMARY KEY,
                  entity_type TEXT NOT NULL,
                  local_id INTEGER,
                  local_sync_id TEXT NOT NULL,
                  remote_kind TEXT NOT NULL,
                  remote_id TEXT NOT NULL,
                  remote_parent_id TEXT,
                  remote_etag TEXT,
                  remote_change_key TEXT,
                  remote_last_modified TEXT,
                  last_synced_at TEXT,
                  deleted_at TEXT
                );
                ",
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO schedule_blocks (id, title) VALUES (1, 'live'), (2, 'deleted')",
                [],
            )
            .expect("blocks");
        connection
            .execute(
                "INSERT INTO sync_meta (entity_type, local_id, sync_id, deleted_at, updated_at)
                 VALUES ('schedule_block', 1, 'schedule_block-1', NULL, 1),
                        ('schedule_block', 2, 'schedule_block-2', 2, 2)",
                [],
            )
            .expect("sync meta");
        connection
            .execute(
                "INSERT INTO feishu_sync_links
                 (id, entity_type, local_id, local_sync_id, remote_kind, remote_id, deleted_at)
                 VALUES
                 (1, 'schedule_block', 1, 'schedule_block-1', 'feishu_calendar_event', 'live', NULL),
                 (2, 'schedule_block', 2, 'schedule_block-2', 'feishu_calendar_event', 'tombstone', NULL),
                 (3, 'schedule_block', 3, 'schedule_block-3', 'feishu_calendar_event', 'missing-local', NULL),
                 (4, 'schedule_block', 4, 'schedule_block-4', 'feishu_calendar_event', 'already-deleted', 'now'),
                 (5, 'checklist_task', 5, 'checklist_task-5', 'feishu_calendar_event', 'other-entity', NULL)",
                [],
            )
            .expect("links");

        let links = load_orphan_calendar_event_links(&connection).expect("loads orphan links");
        let remote_ids = links
            .into_iter()
            .map(|link| link.remote_id)
            .collect::<Vec<_>>();

        assert_eq!(remote_ids, vec!["tombstone", "missing-local"]);
    }

    #[test]
    fn local_calendar_change_pushes_when_remote_timestamp_is_missing() {
        assert_eq!(
            linked_calendar_action(2_000, None, true, false, true),
            LinkedCalendarAction::PushLocal
        );
        assert_eq!(
            linked_calendar_action(6_500, None, true, false, false),
            LinkedCalendarAction::PushLocal
        );
        assert_eq!(
            linked_calendar_action(2_000, None, false, true, false),
            LinkedCalendarAction::PullRemote
        );
        assert_eq!(
            linked_calendar_action(2_000, None, false, false, true),
            LinkedCalendarAction::RefreshLink
        );
    }

    #[test]
    fn calendar_fingerprint_tracks_block_time_changes() {
        let local = LocalScheduleBlock {
            id: 1,
            sync_id: "schedule_block-1".to_string(),
            schedule_date: "2026-05-20".to_string(),
            title: "Math".to_string(),
            note: Some("chapter 3".to_string()),
            start_minute: 600,
            end_minute: 660,
            status: "planned".to_string(),
            updated_at: "2026-05-20T00:00:00Z".to_string(),
            deleted_at: None,
        };
        let remote = RemoteEvent {
            id: "event-1".to_string(),
            title: "Math".to_string(),
            note: Some("chapter 3".to_string()),
            schedule_date: "2026-05-20".to_string(),
            start_minute: 630,
            end_minute: 690,
            updated_millis: None,
            marker_sync_id: Some("schedule_block-1".to_string()),
        };

        assert_ne!(
            local_schedule_block_fingerprint(&local),
            remote_event_fingerprint(&remote)
        );
    }

    #[test]
    #[ignore = "uses the local app database and live Feishu account"]
    fn live_sync_feishu_bridge_once() {
        let trigger =
            std::env::var("FEISHU_LIVE_TEST_TRIGGER").unwrap_or_else(|_| "live_test".to_string());
        let database_path = PathBuf::from(std::env::var("APPDATA").expect("APPDATA is required"))
            .join("com.kaoyan.focus")
            .join("kaoyan-focus.sqlite3");
        let result = sync_feishu_bridge_blocking(
            database_path,
            trigger,
            Uuid::new_v4().to_string(),
            Utc::now(),
        )
        .expect("live Feishu sync should complete");
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serializes sync result")
        );
        assert_ne!(result.status, "failed", "{}", result.message);
    }
}

fn feishu_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("https://") {
        path_or_url.to_string()
    } else {
        format!("{FEISHU_BASE}{path_or_url}")
    }
}


use std::{thread, time::Duration};

use tauri::AppHandle;

use crate::{commands, runtime_health};

pub fn start(app: &AppHandle) {
    commands::sync::prune_sync_backups_best_effort(app);
    runtime_health::mark_task_success("sync_backup_prune", Some(60 * 60));
    start_study_runtime_tick(app.clone());
    start_object_storage_poll(app.clone());
    start_sync_backup_prune(app.clone());
}

fn start_study_runtime_tick(app: AppHandle) {
    thread::spawn(move || loop {
        match commands::focus::tick_background_study_mode(&app) {
            Ok(()) => runtime_health::mark_task_success("study_runtime_tick", Some(3)),
            Err(error) => {
                runtime_health::mark_task_error("study_runtime_tick", &error, Some(3));
            }
        }
        thread::sleep(Duration::from_secs(3));
    });
}

fn start_object_storage_poll(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(10));
        loop {
            eprintln!("Starting desktop background object storage sync poll");
            match commands::sync::poll_object_storage_for_remote_changes(app.clone()) {
                Ok(result) => {
                    eprintln!(
                        "Desktop background object storage sync finished: status={} message={}",
                        result.status, result.message
                    );
                    if result.status == "skipped" {
                        runtime_health::mark_task_success(
                            "object_storage_background_poll",
                            Some(15),
                        );
                    } else {
                        runtime_health::mark_task_success(
                            "object_storage_background_poll",
                            Some(120),
                        );
                    }
                    if result
                        .skipped_reason
                        .as_deref()
                        .is_some_and(|reason| reason == "object_storage_sync_in_flight")
                    {
                        thread::sleep(Duration::from_secs(15));
                        continue;
                    }
                }
                Err(error) => {
                    eprintln!("Desktop background object storage sync failed: {error}");
                    runtime_health::mark_task_error(
                        "object_storage_background_poll",
                        &error,
                        Some(120),
                    );
                }
            }
            thread::sleep(Duration::from_secs(120));
        }
    });
}

fn start_sync_backup_prune(app: AppHandle) {
    thread::spawn(move || loop {
        commands::sync::prune_sync_backups_best_effort(&app);
        runtime_health::mark_task_success("sync_backup_prune", Some(60 * 60));
        thread::sleep(Duration::from_secs(60 * 60));
    });
}

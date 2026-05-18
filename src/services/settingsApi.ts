import type {
  AppSettings,
  ObjectStorageAutoSyncResult,
  ObjectStorageSettings,
  ObjectStorageStatus,
  ObjectStorageSyncResult,
  SyncBackupEntry,
  SyncBackupPreview,
  SyncRunSummary,
  WebDavAutoSyncResult,
  WebDavSettings,
  WebDavStatus,
  WebDavSyncResult,
} from '../types/settings';

export const STUDY_SYNC_STATE_CHANGED_EVENT = 'study-sync-state-changed';

export type AppDataLocation = {
  app_data_dir: string;
  database_path: string;
};

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function getAppSettings(): Promise<AppSettings> {
  return invokeCommand<AppSettings>('get_app_settings');
}

export function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invokeCommand<AppSettings>('save_app_settings', { settings });
}

export function getAppDataLocation(): Promise<AppDataLocation> {
  return invokeCommand<AppDataLocation>('get_app_data_location');
}

export function getWebDavSettings(): Promise<WebDavSettings> {
  return invokeCommand<WebDavSettings>('get_webdav_settings');
}

export function saveWebDavSettings(settings: WebDavSettings): Promise<WebDavSettings> {
  return invokeCommand<WebDavSettings>('save_webdav_settings', { settings });
}

export function testWebDavConnection(settings: WebDavSettings): Promise<WebDavStatus> {
  return invokeCommand<WebDavStatus>('test_webdav_connection', { settings });
}

export function uploadDatabaseToWebDav(settings: WebDavSettings): Promise<WebDavSyncResult> {
  return invokeCommand<WebDavSyncResult>('upload_database_to_webdav', { settings });
}

export function downloadDatabaseFromWebDav(settings: WebDavSettings): Promise<WebDavSyncResult> {
  return invokeCommand<WebDavSyncResult>('download_database_from_webdav', { settings });
}

export function autoSyncWebDavDatabase(): Promise<WebDavAutoSyncResult> {
  return invokeCommand<WebDavAutoSyncResult>('auto_sync_webdav_database');
}

export function getObjectStorageSettings(): Promise<ObjectStorageSettings> {
  return invokeCommand<ObjectStorageSettings>('get_object_storage_settings');
}

export function saveObjectStorageSettings(settings: ObjectStorageSettings): Promise<ObjectStorageSettings> {
  return invokeCommand<ObjectStorageSettings>('save_object_storage_settings', { settings });
}

export function testObjectStorageConnection(settings: ObjectStorageSettings): Promise<ObjectStorageStatus> {
  return invokeCommand<ObjectStorageStatus>('test_object_storage_connection', { settings });
}

export function uploadDatabaseToObjectStorage(settings: ObjectStorageSettings): Promise<ObjectStorageSyncResult> {
  return invokeCommand<ObjectStorageSyncResult>('upload_database_to_object_storage', { settings });
}

export function downloadDatabaseFromObjectStorage(settings: ObjectStorageSettings): Promise<ObjectStorageSyncResult> {
  return invokeCommand<ObjectStorageSyncResult>('download_database_from_object_storage', { settings });
}

export function autoSyncObjectStorageDatabase(): Promise<ObjectStorageAutoSyncResult> {
  return invokeCommand<ObjectStorageAutoSyncResult>('auto_sync_object_storage_database');
}

export function syncObjectStorageStateChange(): Promise<ObjectStorageAutoSyncResult> {
  return invokeCommand<ObjectStorageAutoSyncResult>('sync_object_storage_state_change');
}

export function listSyncRuns(limit = 10): Promise<SyncRunSummary[]> {
  return invokeCommand<SyncRunSummary[]>('list_sync_runs', { limit });
}

export function listSyncBackups(): Promise<SyncBackupEntry[]> {
  return invokeCommand<SyncBackupEntry[]>('list_sync_backups');
}

export function previewSyncBackup(source: string, key: string): Promise<SyncBackupPreview> {
  return invokeCommand<SyncBackupPreview>('preview_sync_backup', { source, key });
}

export function restoreSyncBackup(source: string, key: string): Promise<string> {
  return invokeCommand<string>('restore_sync_backup', { source, key });
}

export async function autoSyncConfiguredDatabase(): Promise<WebDavAutoSyncResult | ObjectStorageAutoSyncResult> {
  const settings = await getAppSettings();
  if (settings.sync_backend === 'object_storage') {
    return autoSyncObjectStorageDatabase();
  }

  return autoSyncWebDavDatabase();
}

export async function syncConfiguredStateChange(): Promise<WebDavAutoSyncResult | ObjectStorageAutoSyncResult> {
  const settings = await getAppSettings();
  if (settings.sync_backend === 'object_storage') {
    return syncObjectStorageStateChange();
  }

  return autoSyncWebDavDatabase();
}

import type { AppSettings, WebDavAutoSyncResult, WebDavSettings, WebDavStatus, WebDavSyncResult } from '../types/settings';

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

import type { AppSettings } from '../types/settings';

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

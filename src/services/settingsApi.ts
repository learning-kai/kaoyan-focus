import type {
  AppSettings,
  ReminderSoundData,
  ReminderSoundFile,
} from '../types/settings';
import type { AppDataLocation } from '../pages/settings/types';
import { invokeCommand } from './tauriInvoke';

export function getAppSettings(): Promise<AppSettings> {
  return invokeCommand<AppSettings>('get_app_settings');
}

export function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invokeCommand<AppSettings>('save_app_settings', { settings });
}

export function saveCustomReminderSound(file: ReminderSoundFile): Promise<AppSettings> {
  return invokeCommand<AppSettings>('save_custom_reminder_sound', { file });
}

export function getCustomReminderSound(): Promise<ReminderSoundData | null> {
  return invokeCommand<ReminderSoundData | null>('get_custom_reminder_sound');
}

export function resetCustomReminderSound(): Promise<AppSettings> {
  return invokeCommand<AppSettings>('reset_custom_reminder_sound');
}

export function getAppDataLocation(): Promise<AppDataLocation> {
  return invokeCommand<AppDataLocation>('get_app_data_location');
}

export function openAppDataLocation(): Promise<AppDataLocation> {
  return invokeCommand<AppDataLocation>('open_app_data_location');
}

export function getSyncDeviceId(): Promise<string> {
  return invokeCommand<string>('get_sync_device_id');
}

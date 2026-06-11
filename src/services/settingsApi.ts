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

/**
 * 跳过指定版本的更新提醒
 */
export async function skipUpdateVersion(version: string): Promise<void> {
  const settings = await getAppSettings();
  await saveAppSettings({
    ...settings,
    skip_update_version: version,
  });
}

/**
 * 设置稍后提醒（指定时间后再次提醒）
 */
export async function snoozeUpdateReminder(durationMs: number): Promise<void> {
  const settings = await getAppSettings();
  const snoozeUntil = Math.floor((Date.now() + durationMs) / 1000); // 转换为秒
  await saveAppSettings({
    ...settings,
    update_reminder_snooze_until: snoozeUntil,
  });
}

/**
 * 清除跳过版本设置
 */
export async function clearSkipUpdateVersion(): Promise<void> {
  const settings = await getAppSettings();
  await saveAppSettings({
    ...settings,
    skip_update_version: null,
  });
}

/**
 * 清除稍后提醒设置
 */
export async function clearUpdateReminderSnooze(): Promise<void> {
  const settings = await getAppSettings();
  await saveAppSettings({
    ...settings,
    update_reminder_snooze_until: null,
  });
}

import type { CalDavCalendar, CalDavSettings, CalDavStatus, CalDavSyncResult } from '../types/settings';
import { invokeCommand } from './tauriInvoke';

export const CALDAV_SYNC_REFRESH_EVENT = 'kaoyan-focus-caldav-sync-refresh';

export function getCalDavSettings(): Promise<CalDavSettings> {
  return invokeCommand<CalDavSettings>('get_caldav_settings');
}

export function saveCalDavSettings(settings: CalDavSettings): Promise<CalDavSettings> {
  return invokeCommand<CalDavSettings>('save_caldav_settings', { settings });
}

export function discoverCalDavCalendars(settings: CalDavSettings): Promise<CalDavCalendar[]> {
  return invokeCommand<CalDavCalendar[]>('discover_caldav_calendars', { settings });
}

export function testCalDavConnection(settings: CalDavSettings): Promise<CalDavStatus> {
  return invokeCommand<CalDavStatus>('test_caldav_connection', { settings });
}

export function syncCalDavCalendar(trigger = 'manual'): Promise<CalDavSyncResult> {
  return invokeCommand<CalDavSyncResult>('sync_caldav_calendar', { trigger });
}

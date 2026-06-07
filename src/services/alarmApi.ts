import type { Alarm, AlarmDraft } from '../types/alarm';
import { invokeCommand } from './tauriInvoke';

export const ALARM_STATE_CHANGED_EVENT = 'alarm-state-changed';

export function listAlarms(): Promise<Alarm[]> {
  return invokeCommand<Alarm[]>('list_alarms');
}

export function createAlarm(draft: AlarmDraft): Promise<Alarm> {
  return invokeCommand<Alarm>('create_alarm', { draft });
}

export function updateAlarm(id: number, draft: AlarmDraft): Promise<Alarm> {
  return invokeCommand<Alarm>('update_alarm', { id, draft });
}

export function deleteAlarm(id: number): Promise<void> {
  return invokeCommand<void>('delete_alarm', { id });
}

export function setAlarmEnabled(id: number, enabled: boolean): Promise<Alarm> {
  return invokeCommand<Alarm>('set_alarm_enabled', { id, enabled });
}

export function dismissAlarm(id: number): Promise<Alarm> {
  return invokeCommand<Alarm>('dismiss_alarm', { id });
}

export function triggerDueAlarms(): Promise<Alarm[]> {
  return invokeCommand<Alarm[]>('trigger_due_alarms');
}

export function getNextAlarm(): Promise<Alarm | null> {
  return invokeCommand<Alarm | null>('get_next_alarm');
}

export function hasActiveAlarm(): Promise<boolean> {
  return invokeCommand<boolean>('has_active_alarm');
}

export function notifyAlarmStateChanged() {
  window.dispatchEvent(new CustomEvent(ALARM_STATE_CHANGED_EVENT));
}

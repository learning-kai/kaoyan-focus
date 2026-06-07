import type { FocusAppCheck, ForegroundApp, InterruptionSummary } from '../types/monitor';
import { invokeCommand } from './tauriInvoke';

export function getCurrentForegroundApp(): Promise<ForegroundApp> {
  return invokeCommand<ForegroundApp>('get_current_foreground_app');
}

export function checkFocusForegroundApp(sessionId: number): Promise<FocusAppCheck> {
  return invokeCommand<FocusAppCheck>('check_focus_foreground_app', { sessionId });
}

export function listInterruptionSummary(): Promise<InterruptionSummary[]> {
  return invokeCommand<InterruptionSummary[]>('list_interruption_summary');
}

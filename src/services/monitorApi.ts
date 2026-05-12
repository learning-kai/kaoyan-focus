import type { FocusAppCheck, ForegroundApp, InterruptionSummary } from '../types/monitor';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function getCurrentForegroundApp(): Promise<ForegroundApp> {
  return invokeCommand<ForegroundApp>('get_current_foreground_app');
}

export function checkFocusForegroundApp(sessionId: number): Promise<FocusAppCheck> {
  return invokeCommand<FocusAppCheck>('check_focus_foreground_app', { sessionId });
}

export function listInterruptionSummary(): Promise<InterruptionSummary[]> {
  return invokeCommand<InterruptionSummary[]>('list_interruption_summary');
}

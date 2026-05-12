import type { FocusMode, FocusSession } from '../types/focus';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function startFocusSession(plannedSeconds: number, mode: FocusMode): Promise<FocusSession> {
  return invokeCommand<FocusSession>('start_focus_session', {
    plannedSeconds,
    mode,
  });
}

export function finishFocusSession(sessionId: number, actualSeconds: number): Promise<FocusSession> {
  return invokeCommand<FocusSession>('finish_focus_session', {
    sessionId,
    actualSeconds,
  });
}

export function listFocusSessions(): Promise<FocusSession[]> {
  return invokeCommand<FocusSession[]>('list_focus_sessions');
}

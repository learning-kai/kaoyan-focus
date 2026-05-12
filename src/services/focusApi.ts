import type { FocusMode, FocusSession, FocusSessionRecovery, FocusStatsSummary, Subject } from '../types/focus';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function startFocusSession(plannedSeconds: number, mode: FocusMode, subjectId?: number | null): Promise<FocusSession> {
  return invokeCommand<FocusSession>('start_focus_session', {
    plannedSeconds,
    mode,
    subjectId,
  });
}

export function setStudyModeActive(active: boolean): Promise<void> {
  return invokeCommand<void>('set_study_mode_active', { active });
}

export function finishFocusSession(sessionId: number, actualSeconds: number): Promise<FocusSession> {
  return invokeCommand<FocusSession>('finish_focus_session', {
    sessionId,
    actualSeconds,
  });
}

export function emergencyExitFocusSession(sessionId: number, actualSeconds: number): Promise<FocusSession> {
  return invokeCommand<FocusSession>('emergency_exit_focus_session', {
    sessionId,
    actualSeconds,
  });
}

export function interruptFocusSession(sessionId: number, actualSeconds: number): Promise<FocusSession> {
  return invokeCommand<FocusSession>('interrupt_focus_session', {
    sessionId,
    actualSeconds,
  });
}

export function recoverActiveFocusSession(): Promise<FocusSessionRecovery | null> {
  return invokeCommand<FocusSessionRecovery | null>('recover_active_focus_session');
}

export function listFocusSessions(): Promise<FocusSession[]> {
  return invokeCommand<FocusSession[]>('list_focus_sessions');
}

export function listSubjects(): Promise<Subject[]> {
  return invokeCommand<Subject[]>('list_subjects');
}

export function getFocusStatsSummary(): Promise<FocusStatsSummary> {
  return invokeCommand<FocusStatsSummary>('get_focus_stats_summary');
}

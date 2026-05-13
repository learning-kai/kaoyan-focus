import type { FocusMode, FocusSession, FocusSessionRecovery, FocusStatsSummary, StudyModeState, Subject } from '../types/focus';

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

export function startStudyMode(
  plannedSeconds: number,
  focusSeconds: number,
  breakSeconds: number,
  longBreakSeconds: number,
  longBreakInterval: number,
  mode: FocusMode,
  subjectId?: number | null,
): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('start_study_mode', {
    plannedSeconds,
    focusSeconds,
    breakSeconds,
    longBreakSeconds,
    longBreakInterval,
    mode,
    subjectId,
  });
}

export function getStudyModeState(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('get_study_mode_state');
}

export function confirmStudyBreak(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('confirm_study_break');
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

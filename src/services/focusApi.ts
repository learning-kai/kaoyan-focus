import type { FocusMode, FocusSession, FocusSessionRecovery, FocusStatsSummary, FocusTimerKind, StudyModeState, Subject } from '../types/focus';
import { invokeCommand } from './tauriInvoke';

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
  whitelistEnabled?: boolean | null,
): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('start_study_mode', {
    plannedSeconds,
    focusSeconds,
    breakSeconds,
    longBreakSeconds,
    longBreakInterval,
    mode,
    subjectId,
    whitelistEnabled,
  });
}

export function startCountupStudyMode(
  breakSeconds: number,
  mode: FocusMode,
  subjectId?: number | null,
  whitelistEnabled?: boolean | null,
): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('start_countup_study_mode', {
    breakSeconds,
    mode,
    subjectId,
    whitelistEnabled,
  });
}

export function takeManualBreak(breakSeconds: number): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('take_manual_break', {
    breakSeconds,
  });
}

export type SwitchTimerKindOptions = {
  plannedSeconds?: number;
  focusSeconds?: number;
  breakSeconds?: number;
  longBreakSeconds?: number;
  longBreakInterval?: number;
};

export function switchStudyTimerKind(kind: FocusTimerKind, options?: SwitchTimerKindOptions): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('switch_study_timer_kind', {
    kind,
    plannedSeconds: options?.plannedSeconds,
    focusSeconds: options?.focusSeconds,
    breakSeconds: options?.breakSeconds,
    longBreakSeconds: options?.longBreakSeconds,
    longBreakInterval: options?.longBreakInterval,
  });
}

export function getStudyModeState(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('get_study_mode_state');
}

export function confirmStudyBreak(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('confirm_study_break');
}

export function skipStudyBreak(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('skip_study_break');
}

export function pauseStudyMode(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('pause_study_mode');
}

export function resumeStudyMode(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('resume_study_mode');
}

export function updateStudyModeSubject(subjectId: number | null): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('update_study_mode_subject', { subjectId });
}

export function updateStudyModeWhitelist(whitelistEnabled: boolean): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('update_study_mode_whitelist', { whitelistEnabled });
}

export function resetStudyMode(): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('reset_study_mode');
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

export function listFocusSessions(limit?: number): Promise<FocusSession[]> {
  return invokeCommand<FocusSession[]>('list_focus_sessions', typeof limit === 'number' ? { limit } : undefined);
}

/**
 * 取出与 [startAt, endAt) 有交集的专注记录，包含仍在进行的那一次。
 * 参数是完整时间戳（UTC），调用方负责按本地时区算出当天起止。
 */
export function listFocusSessionsInRange(startAt: string, endAt: string): Promise<FocusSession[]> {
  return invokeCommand<FocusSession[]>('list_focus_sessions_in_range', {
    startAt,
    endAt,
  });
}

export function deleteFocusSession(sessionId: number): Promise<void> {
  return invokeCommand<void>('delete_focus_session', {
    sessionId,
  });
}

export function updateFocusSessionSubject(sessionId: number, subjectId: number | null): Promise<FocusSession> {
  return invokeCommand<FocusSession>('update_focus_session_subject', {
    sessionId,
    subjectId,
  });
}

export function listSubjects(): Promise<Subject[]> {
  return invokeCommand<Subject[]>('list_subjects');
}

export function getFocusStatsSummary(): Promise<FocusStatsSummary> {
  return invokeCommand<FocusStatsSummary>('get_focus_stats_summary');
}

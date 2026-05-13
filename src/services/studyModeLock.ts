import type { StudyModeState } from '../types/focus';

export function isStudyModeLocked(state: StudyModeState | null) {
  return state?.status === 'active' && ['focus', 'awaiting_break', 'break'].includes(state.phase);
}

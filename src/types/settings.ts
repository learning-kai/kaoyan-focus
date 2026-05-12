import type { FocusMode } from './focus';

export type AppSettings = {
  default_study_minutes: number;
  default_focus_minutes: number;
  break_minutes: number;
  default_focus_mode: FocusMode;
  emergency_cooldown_seconds: number;
};

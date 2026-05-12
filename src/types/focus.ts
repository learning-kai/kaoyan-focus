export type FocusStatus = 'idle' | 'running' | 'finished' | 'interrupted' | 'emergency_exited';

export type FocusMode = 'normal' | 'strict';

export type FocusSession = {
  id: number;
  mode: FocusMode;
  subject_id: number | null;
  planned_seconds: number;
  actual_seconds: number;
  started_at: string;
  ended_at: string | null;
  status: FocusStatus;
  end_reason: string | null;
  interruption_count: number;
  emergency_exit_count: number;
  created_at: string;
  updated_at: string;
};

export type Subject = {
  id: number;
  name: string;
  color: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type SubjectStats = {
  subject: Subject;
  total_seconds: number;
};

export type FocusStatsSummary = {
  today_seconds: number;
  week_seconds: number;
  month_seconds: number;
  interruption_count: number;
  subjects: SubjectStats[];
};

export type FocusSessionRecovery = {
  recovery_status: 'resumed';
  session: FocusSession;
  elapsed_seconds: number;
  remaining_seconds: number;
};

export type StudyModePhase = 'idle' | 'focus' | 'awaiting_break' | 'break' | 'finished' | 'emergency_exited';

export type StudyModeStatus = 'idle' | 'active' | 'finished' | 'emergency_exited';

export type StudyModeState = {
  id: number | null;
  phase: StudyModePhase;
  status: StudyModeStatus;
  mode: FocusMode;
  subject_id: number | null;
  planned_seconds: number;
  focus_seconds: number;
  break_seconds: number;
  cycle_index: number;
  started_at: string | null;
  phase_started_at: string | null;
  ended_at: string | null;
  current_session: FocusSession | null;
  study_elapsed_seconds: number;
  study_remaining_seconds: number;
  phase_elapsed_seconds: number;
  phase_remaining_seconds: number;
  focus_enforcement_active: boolean;
};

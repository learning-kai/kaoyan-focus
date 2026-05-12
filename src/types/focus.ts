export type FocusStatus = 'idle' | 'running' | 'finished' | 'interrupted' | 'emergency_exited';

export type FocusMode = 'normal' | 'strict';

export type FocusSession = {
  id: number;
  mode: FocusMode;
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

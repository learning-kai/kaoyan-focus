export type ForegroundApp = {
  process_id: number;
  process_name: string;
  process_path: string | null;
  window_title: string;
};

export type RunningProcess = {
  process_id: number;
  process_name: string;
  process_path: string | null;
};

export type WhitelistMatchResult = {
  allowed: boolean;
  reason: string;
  matched_process_name: string | null;
};

export type FocusAppCheck = {
  foreground_app: ForegroundApp;
  match_result: WhitelistMatchResult;
  interruption_count: number;
  action_taken: string | null;
  close_error: string | null;
};

export type InterruptionSummary = {
  process_name: string;
  process_path: string | null;
  window_title: string | null;
  interruption_count: number;
  last_interrupted_at: string;
};

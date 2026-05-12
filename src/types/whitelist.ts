export type WhitelistApp = {
  id: number;
  name: string;
  process_name: string;
  path: string | null;
  match_type: string;
  note: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type RunningProcess = {
  process_id: number;
  process_name: string;
  process_path: string | null;
};

export type RecentBlockedApp = {
  process_name: string;
  process_path: string | null;
  window_title: string | null;
  blocked_count: number;
  last_blocked_at: string;
};

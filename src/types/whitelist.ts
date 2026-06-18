export type WhitelistApp = {
  id: number;
  name: string;
  process_name: string;
  path: string | null;
  match_type: 'process_name' | 'website_domain' | 'potplayer_video_file' | 'potplayer_video_directory' | string;
  list_kind: 'allowlist' | 'blocklist' | string;
  subject_id: number | null;
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

export type PotPlayerMediaInfo = {
  process_name: string;
  media_path: string | null;
  media_directory: string | null;
  window_title: string;
  source: string | null;
};

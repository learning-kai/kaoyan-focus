import type { FocusMode } from './focus';

export type AppSettings = {
  default_study_minutes: number;
  default_focus_minutes: number;
  break_minutes: number;
  long_break_minutes: number;
  long_break_interval: number;
  default_focus_mode: FocusMode;
  emergency_cooldown_seconds: number;
};

export type WebDavSettings = {
  url: string;
  username: string;
  password: string;
  remote_path: string;
};

export type WebDavStatus = {
  configured: boolean;
  url: string;
  username: string;
  remote_path: string;
  remote_exists: boolean;
  remote_size: number | null;
  last_modified: string | null;
  message: string;
};

export type WebDavSyncResult = {
  success: boolean;
  message: string;
  remote_url: string;
  bytes: number;
  backup_path: string | null;
};

export type WebDavAutoSyncResult = {
  status: 'synced' | 'skipped';
  message: string;
  direction: 'upload' | 'download_upload' | null;
  skipped_reason: string | null;
  synced_at: string;
  remote_url: string | null;
  bytes: number;
  backup_path: string | null;
};

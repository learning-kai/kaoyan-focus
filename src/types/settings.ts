import type { FocusMode } from './focus';

export type AppTheme = 'dark' | 'light';
export type SyncBackend = 'webdav' | 'object_storage';

export type AppSettings = {
  default_study_minutes: number;
  default_focus_minutes: number;
  break_minutes: number;
  long_break_minutes: number;
  long_break_interval: number;
  default_focus_mode: FocusMode;
  ui_theme: AppTheme;
  sync_backend: SyncBackend;
  emergency_cooldown_seconds: number;
  checklist_category_names: string;
};

export type WebDavSettings = {
  enabled: boolean;
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
  active_state_changed?: boolean;
  took_over_active_mode?: boolean;
};

export type ObjectStorageSettings = {
  enabled: boolean;
  endpoint: string;
  bucket: string;
  access_key_id: string;
  secret_access_key: string;
  region: string;
  object_key: string;
};

export type ObjectStorageStatus = {
  configured: boolean;
  endpoint: string;
  bucket: string;
  region: string;
  object_key: string;
  object_exists: boolean;
  object_size: number | null;
  last_modified: string | null;
  message: string;
};

export type ObjectStorageSyncResult = {
  success: boolean;
  message: string;
  object_url: string;
  bytes: number;
  backup_path: string | null;
};

export type ObjectStorageAutoSyncResult = {
  status: 'synced' | 'skipped';
  message: string;
  direction: 'upload' | 'download_upload' | null;
  skipped_reason: string | null;
  synced_at: string;
  object_url: string | null;
  bytes: number;
  backup_path: string | null;
  active_state_changed?: boolean;
  took_over_active_mode?: boolean;
};

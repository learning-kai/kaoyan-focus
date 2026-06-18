import type { FocusMode } from './focus';

export type AppTheme = 'dark' | 'light' | 'mono' | 'dawn' | 'forest' | 'sakura';
export type SyncBackend = 'webdav' | 'object_storage';
export type ReminderSoundSource = 'builtin' | 'custom';
export type ReminderSoundId = 'classic' | 'bright' | 'soft' | 'urgent' | 'short';
export type RuntimeHealthStatus = 'ok' | 'healthy' | 'warning' | 'degraded' | 'error' | 'failed' | 'unavailable' | 'unknown';

export type AppSettings = {
  default_study_minutes: number;
  default_focus_minutes: number;
  break_minutes: number;
  long_break_minutes: number;
  long_break_interval: number;
  default_focus_mode: FocusMode;
  whitelist_mode: 'allowlist' | 'blocklist';
  ui_theme: AppTheme;
  launch_at_startup: boolean;
  auto_start_break_after_focus: boolean;
  schedule_reminder_enabled: boolean;
  schedule_reminder_lead_minutes: number;
  focus_widget_enabled: boolean;
  focus_widget_auto_follow: boolean;
  focus_widget_remember_geometry: boolean;
  focus_widget_always_on_top: boolean;
  focus_widget_x: number | null;
  focus_widget_y: number | null;
  focus_widget_width: number | null;
  focus_widget_height: number | null;
  sync_backend: SyncBackend;
  primary_owner_device_id: string | null;
  primary_owner_updated_at: number | null;
  emergency_cooldown_seconds: number;
  checklist_category_names: string;
  reminder_sound_source: ReminderSoundSource;
  reminder_sound_id: ReminderSoundId;
  reminder_sound_file_name: string | null;
  reminder_sound_updated_at: number | null;
  reminder_sound_volume: number;
  reminder_sound_duration_seconds: number;
  reminder_quiet_hours_enabled: boolean;
  reminder_quiet_hours_start: string;
  reminder_quiet_hours_end: string;
  auto_download_update: boolean;
  skip_update_version: string | null;
  update_reminder_snooze_until: number | null;
};

export type ReminderSoundFile = {
  fileName: string;
  bytes: number[];
};

export type ReminderSoundData = {
  file_name: string;
  mime_type: string;
  bytes: number[];
};

export type WebDavSettings = {
  enabled: boolean;
  url: string;
  username: string;
  password: string;
  password_configured?: boolean;
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
  primary_owner_changed?: boolean;
};

export type ObjectStorageSettings = {
  enabled: boolean;
  endpoint: string;
  bucket: string;
  access_key_id: string;
  secret_access_key: string;
  secret_access_key_configured?: boolean;
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
  direction: 'upload' | 'download_upload' | 'download' | null;
  skipped_reason: string | null;
  synced_at: string;
  object_url: string | null;
  bytes: number;
  backup_path: string | null;
  active_state_changed?: boolean;
  took_over_active_mode?: boolean;
  primary_owner_changed?: boolean;
};

export type SyncRunSummary = {
  id: number;
  sync_id: string;
  backend: string;
  trigger: string;
  direction: string | null;
  status: string;
  started_at: string;
  finished_at: string;
  duration_ms: number;
  bytes: number;
  imported_count: number;
  exported_count: number;
  deleted_count: number;
  conflict_count: number;
  active_state_changed: boolean;
  took_over_active_mode: boolean;
  validation_report: string | null;
  backup_path: string | null;
  remote_backup_key: string | null;
  active_snapshot_sync_id: string | null;
  remote_active_snapshot_sync_id: string | null;
  active_snapshot_phase: string | null;
  remote_active_snapshot_phase: string | null;
  active_snapshot_updated_at: number | null;
  remote_snapshot_updated_at: number | null;
  remote_exported_drift_seconds: number | null;
  detail: string | null;
  error_message: string | null;
};

export type SyncBackupEntry = {
  source: string;
  key: string;
  label: string;
  created_at: string | null;
  bytes: number | null;
};

export type SyncBackupPreview = {
  source: string;
  key: string;
  bytes: number;
  validation_report: string;
  entity_count: number;
  deleted_count: number;
  exported_at: number | null;
  device_id: string | null;
};

export type EmailReminderSettings = {
  enabled: boolean;
  smtp_host: string;
  smtp_port: number;
  smtp_security: 'tls' | 'starttls' | 'none';
  username: string;
  password: string;
  password_configured?: boolean;
  from: string;
  to: string;
};

export type EmailReminderResult = {
  status: 'sent' | 'skipped';
  message: string;
  sent_count: number;
};

export type FeishuSyncSettings = {
  enabled: boolean;
  app_id: string;
  app_secret: string;
  app_secret_configured?: boolean;
  redirect_uri: string;
};

export type FeishuSyncRunSummary = {
  id: number;
  run_id: string;
  trigger: string;
  status: string;
  started_at: string;
  finished_at: string;
  duration_ms: number;
  pushed_count: number;
  pulled_count: number;
  deleted_count: number;
  conflict_count: number;
  task_count: number;
  calendar_count: number;
  message: string;
  error_message: string | null;
};

export type FeishuTasklistStatus = {
  key: string;
  label: string;
  guid: string | null;
  ready: boolean;
};

export type FeishuSyncStatus = {
  enabled: boolean;
  configured: boolean;
  authenticated: boolean;
  expires_at: string | null;
  tasklist_guid: string | null;
  tasklist_count: number;
  tasklists: FeishuTasklistStatus[];
  calendar_id: string | null;
  redirect_uri: string;
  pending_authorization_url: string | null;
  pending_message: string | null;
  required_scopes: string;
  last_run: FeishuSyncRunSummary | null;
};

export type FeishuOAuthLogin = {
  status: string;
  authorization_url: string;
  redirect_uri: string;
  message: string;
};

export type FeishuLoginPollResult = {
  status: string;
  message: string;
  authenticated: boolean;
};

export type FeishuSyncResult = {
  status: 'synced' | 'skipped' | 'failed';
  message: string;
  pushed_count: number;
  pulled_count: number;
  deleted_count: number;
  conflict_count: number;
  task_count: number;
  calendar_count: number;
  synced_at: string;
};

export type FeishuRebuildResult = {
  status: string;
  message: string;
  backup_path: string;
  remote_backup_path: string;
  deleted_tasklist_count: number;
  uploaded_task_count: number;
  tasklist_count: number;
  synced_at: string;
};

export type CalDavSettings = {
  enabled: boolean;
  server_url: string;
  username: string;
  password: string;
  password_configured?: boolean;
  selected_calendar_url: string;
  selected_calendar_name: string;
};

export type CalDavCalendar = {
  url: string;
  name: string;
  writable: boolean;
};

export type CalDavStatus = {
  configured: boolean;
  calendar_url: string;
  calendar_name: string;
  message: string;
};

export type CalDavSyncResult = {
  status: 'synced' | 'skipped' | 'failed' | string;
  message: string;
  pushed_count: number;
  pulled_count: number;
  deleted_count: number;
  conflict_count: number;
  event_count: number;
  synced_at: string;
};

export type RuntimeHealthCheck = {
  key?: string;
  label?: string;
  status: RuntimeHealthStatus;
  message?: string | null;
  detail?: string | null;
  checked_at?: string | null;
};

export type RuntimeHealth = {
  status: RuntimeHealthStatus;
  summary?: string | null;
  checked_at?: string | null;
  protected_storage?: RuntimeHealthCheck | null;
  checks?: RuntimeHealthCheck[];
  generated_at?: string;
  tasks?: RuntimeTaskHealth[];
};

export type RuntimeTaskHealth = {
  task: string;
  status: string;
  last_success_at: string | null;
  last_error: string | null;
  next_retry_at: string | null;
};

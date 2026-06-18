export type SettingsPanelKey =
  | 'webdav'
  | 'feishu'
  | 'caldav'
  | 'email'
  | 'syncJournal'
  | 'backups'
  | 'objectStorage'
  | 'rules'
  | 'update'
  | 'foreground'
  | 'runtimeHealth'
  | 'privacyData';

export type AppDataLocation = {
  app_data_dir: string;
  database_path: string;
};

export type WebDavBusyAction = 'save' | 'test' | 'upload' | 'download';

export type ObjectStorageBusyAction =
  | 'save'
  | 'test'
  | 'upload'
  | 'download'
  | 'previewBackup'
  | 'restoreBackup';

export type CalDavBusyAction = 'save' | 'discover' | 'test' | 'sync';

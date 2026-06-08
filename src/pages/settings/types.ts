export type SettingsPanelKey =
  | 'webdav'
  | 'feishu'
  | 'email'
  | 'syncJournal'
  | 'backups'
  | 'objectStorage'
  | 'rules'
  | 'update'
  | 'foreground'
  | 'runtimeHealth'
  | 'privacyData';

export type WebDavBusyAction = 'save' | 'test' | 'upload' | 'download';

export type ObjectStorageBusyAction =
  | 'save'
  | 'test'
  | 'upload'
  | 'download'
  | 'previewBackup'
  | 'restoreBackup';

import { useEffect, useState, type ChangeEvent, type CSSProperties } from 'react';
import {
  Cloud,
  ExternalLink,
  HardDrive,
  RefreshCw,
  Settings2,
  type LucideIcon,
} from 'lucide-react';
import { BasicSettingsPanel } from './settings/BasicSettingsPanel';
import { IntegrationsPanel } from './settings/IntegrationsPanel';
import { SyncSettingsPanel } from './settings/SyncSettingsPanel';
import { SystemPanel } from './settings/SystemPanel';
import { formatBytes } from './settings/SettingsPrimitives';
import type { SettingsPanelKey } from './settings/types';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { getStudyModeState } from '../services/focusApi';
import { getCurrentForegroundApp } from '../services/monitorApi';
import { isStudyModeLocked } from '../services/studyModeLock';
import { previewReminderSound } from '../services/alertApi';
import {
  getAppDataLocation,
  getAppSettings,
  saveAppSettings,
  resetCustomReminderSound,
  saveCustomReminderSound,
} from '../services/settingsApi';
import {
  downloadDatabaseFromObjectStorage,
  downloadDatabaseFromWebDav,
  getObjectStorageSettings,
  getWebDavSettings,
  listSyncBackups,
  listSyncRuns,
  previewSyncBackup,
  restoreSyncBackup,
  saveObjectStorageSettings,
  saveWebDavSettings,
  syncConfiguredStateChange,
  testObjectStorageConnection,
  testWebDavConnection,
  uploadDatabaseToObjectStorage,
  uploadDatabaseToWebDav,
} from '../services/syncApi';
import {
  getFeishuSyncSettings,
  getFeishuSyncStatus,
  logoutFeishu,
  pollFeishuOAuthLogin,
  rebuildFeishuTasklistsFromLocal,
  saveFeishuSyncSettings,
  startFeishuOAuthLogin,
  syncFeishuBridge,
} from '../services/feishuApi';
import {
  getEmailReminderSettings,
  saveEmailReminderSettings,
  testEmailReminder,
} from '../services/emailApi';
import { openExternalUrl } from '../services/systemApi';
import { checkForAppUpdate, installAppUpdate, type AppUpdate } from '../services/updateApi';
import type { StudyModeState } from '../types/focus';
import type { ForegroundApp } from '../types/monitor';
import type {
  AppSettings,
  AppTheme,
  EmailReminderSettings,
  FeishuSyncSettings,
  FeishuSyncStatus,
  ObjectStorageSettings,
  SyncBackupEntry,
  SyncBackend,
  SyncRunSummary,
  ReminderSoundId,
  ReminderSoundSource,
  WebDavSettings,
} from '../types/settings';

const defaultSettings: AppSettings = {
  default_study_minutes: 120,
  default_focus_minutes: 25,
  break_minutes: 5,
  long_break_minutes: 15,
  long_break_interval: 4,
  default_focus_mode: 'normal',
  ui_theme: 'dark',
  launch_at_startup: false,
  auto_start_break_after_focus: false,
  schedule_reminder_enabled: true,
  schedule_reminder_lead_minutes: 5,
  sync_backend: 'webdav',
  primary_owner_device_id: null,
  primary_owner_updated_at: null,
  emergency_cooldown_seconds: 60,
  checklist_category_names: '{"politics":"政治","english":"英语","math":"数学","major":"专业课","general":"通用"}',
  reminder_sound_source: 'builtin',
  reminder_sound_id: 'classic',
  reminder_sound_file_name: null,
  reminder_sound_updated_at: null,
  reminder_sound_volume: 100,
  reminder_quiet_hours_enabled: false,
  reminder_quiet_hours_start: '22:30',
  reminder_quiet_hours_end: '07:00',
};

const defaultWebDavSettings: WebDavSettings = {
  enabled: true,
  url: '',
  username: '',
  password: '',
  remote_path: 'kaoyan-focus/kaoyan-focus.sqlite3',
};

const defaultObjectStorageSettings: ObjectStorageSettings = {
  enabled: false,
  endpoint: '',
  bucket: '',
  access_key_id: '',
  secret_access_key: '',
  region: '',
  object_key: 'study-sync.json',
};

const defaultEmailReminderSettings: EmailReminderSettings = {
  enabled: false,
  smtp_host: '',
  smtp_port: 465,
  smtp_security: 'tls',
  username: '',
  password: '',
  from: '',
  to: '',
};

const defaultFeishuSettings: FeishuSyncSettings = {
  enabled: false,
  app_id: '',
  app_secret: '',
  redirect_uri: 'http://127.0.0.1:39781/feishu/callback',
};

const reminderSoundSourceOptions: Array<{ value: ReminderSoundSource; label: string; description: string }> = [
  {
    value: 'builtin',
    label: '内置',
    description: '直接使用应用自带音色。',
  },
  {
    value: 'custom',
    label: '自定义',
    description: '上传你自己的音频文件。',
  },
];

const reminderSoundOptions: Array<{ id: ReminderSoundId; label: string; description: string }> = [
  {
    id: 'classic',
    label: '经典',
    description: '均衡，适合日常提醒。',
  },
  {
    id: 'bright',
    label: '清亮',
    description: '更清脆，更容易被注意到。',
  },
  {
    id: 'soft',
    label: '柔和',
    description: '节制一点，不会太刺耳。',
  },
  {
    id: 'urgent',
    label: '紧急',
    description: '更强烈一些，适合到点提醒。',
  },
  {
    id: 'short',
    label: '短促',
    description: '短一点，响完就停。',
  },
];

type SettingsPageProps = {
  lastAutoSyncMessage?: string | null;
  lastAutoUpdateMessage?: string | null;
  theme?: AppTheme;
  onThemeChange?: (theme: AppTheme) => void;
};

type SettingsSectionKey = 'basic' | 'sync' | 'integrations' | 'system';

const settingsSections: Array<{ key: SettingsSectionKey; label: string; description: string; icon: LucideIcon }> = [
  { key: 'basic', label: '基础', description: '节奏与提醒', icon: Settings2 },
  { key: 'sync', label: '同步', description: '云端数据', icon: Cloud },
  { key: 'integrations', label: '集成', description: '飞书与邮件', icon: ExternalLink },
  { key: 'system', label: '系统', description: '规则与诊断', icon: HardDrive },
];

export default function SettingsPage({
  lastAutoSyncMessage = null,
  lastAutoUpdateMessage = null,
  theme = 'dark',
  onThemeChange = () => {},
}: SettingsPageProps) {
  const { confirm, confirmDialog } = useConfirmDialog();
  const [foregroundApp, setForegroundApp] = useState<ForegroundApp | null>(null);
  const [studyState, setStudyState] = useState<StudyModeState | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [webDavSettings, setWebDavSettings] = useState<WebDavSettings>(defaultWebDavSettings);
  const [objectStorageSettings, setObjectStorageSettings] = useState<ObjectStorageSettings>(defaultObjectStorageSettings);
  const [emailSettings, setEmailSettings] = useState<EmailReminderSettings>(defaultEmailReminderSettings);
  const [feishuSettings, setFeishuSettings] = useState<FeishuSyncSettings>(defaultFeishuSettings);
  const [feishuStatus, setFeishuStatus] = useState<FeishuSyncStatus | null>(null);
  const [dataLocation, setDataLocation] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const [webDavMessage, setWebDavMessage] = useState<string | null>(null);
  const [objectStorageMessage, setObjectStorageMessage] = useState<string | null>(null);
  const [emailMessage, setEmailMessage] = useState<string | null>(null);
  const [feishuMessage, setFeishuMessage] = useState<string | null>(null);
  const [reminderSoundMessage, setReminderSoundMessage] = useState<string | null>(null);
  const [syncRuns, setSyncRuns] = useState<SyncRunSummary[]>([]);
  const [syncBackups, setSyncBackups] = useState<SyncBackupEntry[]>([]);
  const [syncDetailMessage, setSyncDetailMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AppUpdate | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [webDavBusy, setWebDavBusy] = useState(false);
  const [objectStorageBusy, setObjectStorageBusy] = useState(false);
  const [emailBusy, setEmailBusy] = useState(false);
  const [feishuBusy, setFeishuBusy] = useState(false);
  const [reminderSoundBusy, setReminderSoundBusy] = useState(false);
  const [customReminderSoundFile, setCustomReminderSoundFile] = useState<File | null>(null);
  const [customReminderSoundInputKey, setCustomReminderSoundInputKey] = useState(0);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [activeSection, setActiveSection] = useState<SettingsSectionKey>('basic');
  const [expandedPanels, setExpandedPanels] = useState<Record<SettingsPanelKey, boolean>>({
    webdav: false,
    feishu: false,
    email: false,
    syncJournal: false,
    backups: false,
    objectStorage: false,
    rules: false,
    update: false,
    foreground: false,
  });

  const settingsLocked = isStudyModeLocked(studyState);
  const visibleUpdateMessage = updateMessage;

  useEffect(() => {
    void initializeSettingsPage();
  }, []);

  useEffect(() => {
    setSettings((current) => ({ ...current, ui_theme: theme }));
  }, [theme]);

  useEffect(() => {
    if (!settingsLoaded) {
      return;
    }

    setExpandedPanels((current) => ({
      ...current,
      webdav: current.webdav || settings.sync_backend === 'webdav',
      objectStorage: current.objectStorage || settings.sync_backend === 'object_storage' || objectStorageSettings.enabled,
      feishu: current.feishu || feishuSettings.enabled,
      email: current.email || emailSettings.enabled,
    }));
  }, [settingsLoaded, objectStorageSettings.enabled, settings.sync_backend, feishuSettings.enabled, emailSettings.enabled]);

  useEffect(() => {
    if (!settingsLocked) {
      return;
    }

    const intervalId = window.setInterval(() => {
      void refreshStudyState();
    }, 5000);

    return () => window.clearInterval(intervalId);
  }, [settingsLocked]);

  async function initializeSettingsPage() {
    setSettingsLoaded(false);
    await Promise.all([
      refreshStudyState(),
      refreshSettings(),
      refreshDataLocation(),
      refreshWebDavSettings(),
      refreshObjectStorageSettings(),
      refreshEmailSettings(),
      refreshFeishuSettings(),
      refreshSyncDetails(),
    ]);
    setSettingsLoaded(true);
  }

  async function refreshStudyState() {
    try {
      setStudyState(await getStudyModeState());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshSettings() {
    try {
      setError(null);
      const nextSettings = await getAppSettings();
      setSettings({ ...nextSettings, ui_theme: theme });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshWebDavSettings() {
    try {
      setError(null);
      setWebDavSettings(await getWebDavSettings());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshObjectStorageSettings() {
    try {
      setError(null);
      setObjectStorageSettings(await getObjectStorageSettings());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshEmailSettings() {
    try {
      setError(null);
      setEmailSettings(await getEmailReminderSettings());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshFeishuSettings() {
    try {
      setError(null);
      const [settings, status] = await Promise.all([
        getFeishuSyncSettings(),
        getFeishuSyncStatus(),
      ]);
      setFeishuSettings(settings);
      setFeishuStatus(status);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshDataLocation() {
    try {
      setError(null);
      const location = await getAppDataLocation();
      setDataLocation(location.database_path);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshSyncDetails() {
    try {
      const [runs, backups] = await Promise.all([listSyncRuns(6), listSyncBackups()]);
      setSyncRuns(runs);
      setSyncBackups(backups);
    } catch {
      // Sync detail is observational; keep settings usable if a backup provider is offline.
    }
  }

  async function handleSaveSettings() {
    try {
      setSaving(true);
      setError(null);
      setSavedMessage(null);
      const saved = await saveAppSettings({ ...settings, ui_theme: theme });
      setSettings(saved);
      onThemeChange(saved.ui_theme);
      setSavedMessage('设置已保存，下一次进入专注页会自动使用。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function handleSaveWebDavSettings() {
    await runWebDavAction(async () => {
      const saved = await saveWebDavSettings(webDavSettings);
      setWebDavSettings(saved);
      return 'WebDAV 配置已保存。';
    });
  }

  async function handleTestWebDav() {
    await runWebDavAction(async () => {
      const status = await testWebDavConnection(webDavSettings);
      return status.message;
    });
  }

  async function handleUploadWebDav() {
    await runWebDavAction(async () => {
      const result = await uploadDatabaseToWebDav(webDavSettings);
      return `${result.message} 上传 ${formatBytes(result.bytes)}。`;
    });
  }

  async function handleDownloadWebDav() {
    await runWebDavAction(async () => {
      const result = await downloadDatabaseFromWebDav(webDavSettings);
      return result.backup_path
        ? `${result.message} 备份路径：${result.backup_path}`
        : result.message;
    });
    await initializeSettingsPage();
  }

  async function handleSaveObjectStorageSettings() {
    await runObjectStorageAction(async () => {
      const saved = await saveObjectStorageSettings(objectStorageSettings);
      setObjectStorageSettings(saved);
      return '对象存储配置已保存。';
    });
  }

  async function handleTestObjectStorage() {
    await runObjectStorageAction(async () => {
      const status = await testObjectStorageConnection(objectStorageSettings);
      return status.object_exists
        ? `${status.message} 远端大小 ${status.object_size ? formatBytes(status.object_size) : '未知'}。`
        : status.message;
    });
  }

  async function handleUploadObjectStorage() {
    await runObjectStorageAction(async () => {
      const result = await uploadDatabaseToObjectStorage(objectStorageSettings);
      return `${result.message} 上传 ${formatBytes(result.bytes)}。`;
    });
  }

  async function handleDownloadObjectStorage() {
    await runObjectStorageAction(async () => {
      const result = await downloadDatabaseFromObjectStorage(objectStorageSettings);
      return result.backup_path
        ? `${result.message} 备份路径：${result.backup_path}`
        : result.message;
    });
    await initializeSettingsPage();
  }

  async function handlePreviewBackup(entry: SyncBackupEntry) {
    await runObjectStorageAction(async () => {
      const preview = await previewSyncBackup(entry.source, entry.key);
      setSyncDetailMessage(`${entry.label}：${preview.validation_report}`);
      return '备份预检完成。';
    });
  }

  async function handleRestoreBackup(entry: SyncBackupEntry) {
    const confirmed = await confirm({
      confirmLabel: '恢复备份',
      message: `恢复「${entry.label}」前会先备份当前本地数据库。恢复完成后，页面会重新载入最新配置和同步记录。`,
      title: '恢复同步备份？',
      tone: 'danger',
    });
    if (!confirmed) return;

    await runObjectStorageAction(async () => {
      const message = await restoreSyncBackup(entry.source, entry.key);
      await refreshSyncDetails();
      return message;
    });
    await initializeSettingsPage();
  }

  async function runWebDavAction(action: () => Promise<string>) {
    try {
      setWebDavBusy(true);
      setError(null);
      setWebDavMessage(null);
      setWebDavMessage(await action());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setWebDavBusy(false);
    }
  }

  async function runObjectStorageAction(action: () => Promise<string>) {
    try {
      setObjectStorageBusy(true);
      setError(null);
      setObjectStorageMessage(null);
      setObjectStorageMessage(await action());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setObjectStorageBusy(false);
    }
  }

  async function runReminderSoundAction(action: () => Promise<string>) {
    try {
      setReminderSoundBusy(true);
      setError(null);
      setReminderSoundMessage(null);
      setReminderSoundMessage(await action());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setReminderSoundBusy(false);
    }
  }

  function updateSettings(patch: Partial<AppSettings>) {
    setSavedMessage(null);
    setSettings((current) => ({ ...current, ...patch }));
  }

  function updateWebDavSettings(patch: Partial<WebDavSettings>) {
    setWebDavMessage(null);
    setWebDavSettings((current) => ({ ...current, ...patch }));
  }

  function updateObjectStorageSettings(patch: Partial<ObjectStorageSettings>) {
    setObjectStorageMessage(null);
    setObjectStorageSettings((current) => ({ ...current, ...patch }));
  }

  function updateEmailSettings(patch: Partial<EmailReminderSettings>) {
    setEmailMessage(null);
    setEmailSettings((current) => ({ ...current, ...patch }));
  }

  function updateFeishuSettings(patch: Partial<FeishuSyncSettings>) {
    setFeishuMessage(null);
    setFeishuSettings((current) => ({ ...current, ...patch }));
  }

  function updateReminderSoundSettings(patch: Partial<Pick<AppSettings, 'reminder_sound_source' | 'reminder_sound_id' | 'reminder_sound_volume'>>) {
    setReminderSoundMessage(null);
    updateSettings(patch);
  }

  function handleReminderSoundSourceChange(source: ReminderSoundSource) {
    updateReminderSoundSettings({ reminder_sound_source: source });

    if (source === 'builtin') {
      setCustomReminderSoundFile(null);
      setCustomReminderSoundInputKey((current) => current + 1);
    }
  }

  function handleReminderSoundFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0] ?? null;
    if (!file) {
      return;
    }

    setCustomReminderSoundFile(file);
    handleReminderSoundSourceChange('custom');
  }

  function updateSyncBackend(sync_backend: SyncBackend) {
    updateSettings({ sync_backend });
  }

  function togglePanel(panel: SettingsPanelKey) {
    setExpandedPanels((current) => ({ ...current, [panel]: !current[panel] }));
  }

  const webDavActionDisabled = webDavBusy || settingsLocked || !webDavSettings.enabled;
  const objectStorageActionDisabled = objectStorageBusy || settingsLocked || !objectStorageSettings.enabled;
  const emailActionDisabled = emailBusy || settingsLocked || !emailSettings.enabled;
  const feishuActionDisabled = feishuBusy || settingsLocked || !feishuSettings.enabled;
  const reminderSoundActionDisabled = reminderSoundBusy || settingsLocked;
  const currentReminderSoundOption = reminderSoundOptions.find((option) => option.id === settings.reminder_sound_id) ?? reminderSoundOptions[0];
  const currentReminderSoundSourceOption = reminderSoundSourceOptions.find((option) => option.value === settings.reminder_sound_source) ?? reminderSoundSourceOptions[0];
  const reminderSoundVolumeStyle = {
    '--sound-volume-percent': `${settings.reminder_sound_volume}%`,
  } as CSSProperties;

  async function handleDetectForegroundApp() {
    try {
      setLoading(true);
      setError(null);
      setForegroundApp(await getCurrentForegroundApp());
      await refreshStudyState();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleCheckUpdate() {
    try {
      setCheckingUpdate(true);
      setError(null);
      setUpdateMessage(null);
      setUpdateProgress(null);
      const update = await checkForAppUpdate();
      setAvailableUpdate(update);

      if (update === null) {
        setUpdateMessage('当前已经是最新版本。');
        return;
      }

      setUpdateMessage(`发现新版：${update.version}${update.body ? `，${update.body}` : ''}`);
    } catch (reason) {
      setAvailableUpdate(null);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function handleInstallUpdate() {
    if (availableUpdate === null) {
      return;
    }

    try {
      setInstallingUpdate(true);
      setError(null);
      setUpdateMessage('正在下载更新...');
      await installAppUpdate(availableUpdate, ({ downloadedBytes, totalBytes }) => {
        if (totalBytes && totalBytes > 0) {
          setUpdateProgress(Math.round((downloadedBytes / totalBytes) * 100));
        }
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setInstallingUpdate(false);
    }
  }

  async function handlePreviewReminderSound() {
    await runReminderSoundAction(async () => {
      await previewReminderSound(settings, customReminderSoundFile ?? undefined);
      return '已开始试听提醒音效。';
    });
  }

  async function handleUploadReminderSound() {
    if (!customReminderSoundFile) {
      return;
    }

    await runReminderSoundAction(async () => {
      const bytes = Array.from(new Uint8Array(await customReminderSoundFile.arrayBuffer()));
      const saved = await saveCustomReminderSound({
        fileName: customReminderSoundFile.name,
        bytes,
      });
      setSettings((current) => ({
        ...current,
        ...saved,
        ui_theme: theme,
      }));
      setCustomReminderSoundFile(null);
      setCustomReminderSoundInputKey((current) => current + 1);
      return '自定义提醒音频已上传并启用。';
    });
  }

  async function handleResetReminderSound() {
    await runReminderSoundAction(async () => {
      const saved = await resetCustomReminderSound();
      setSettings((current) => ({
        ...current,
        ...saved,
        ui_theme: theme,
      }));
      setCustomReminderSoundFile(null);
      setCustomReminderSoundInputKey((current) => current + 1);
      return '已恢复默认提醒音效。';
    });
  }

  async function runEmailAction(action: () => Promise<string>) {
    try {
      setEmailBusy(true);
      setError(null);
      setEmailMessage(null);
      setEmailMessage(await action());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setEmailBusy(false);
    }
  }

  async function handleSaveEmailSettings() {
    await runEmailAction(async () => {
      const saved = await saveEmailReminderSettings(emailSettings);
      setEmailSettings(saved);
      return '邮件提醒配置已保存。';
    });
  }

  async function handleTestEmail() {
    await runEmailAction(async () => {
      const result = await testEmailReminder(emailSettings);
      return result.message;
    });
  }

  async function runFeishuAction(action: () => Promise<string>) {
    try {
      setFeishuBusy(true);
      setError(null);
      setFeishuMessage(null);
      setFeishuMessage(await action());
      await refreshFeishuSettings();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setFeishuBusy(false);
    }
  }

  async function handleSaveFeishuSettings() {
    await runFeishuAction(async () => {
      const saved = await saveFeishuSyncSettings(feishuSettings);
      setFeishuSettings(saved);
      return '飞书同步配置已保存。';
    });
  }

  async function handleStartFeishuLogin() {
    await runFeishuAction(async () => {
      await saveFeishuSyncSettings(feishuSettings);
      const login = await startFeishuOAuthLogin();
      await openExternalUrl(login.authorization_url);
      return login.message;
    });
  }

  async function handlePollFeishuLogin() {
    await runFeishuAction(async () => {
      const result = await pollFeishuOAuthLogin();
      return result.message;
    });
  }

  async function handleLogoutFeishu() {
    await runFeishuAction(async () => {
      await logoutFeishu();
      return '飞书登录状态已清除。';
    });
  }

  async function handleSyncFeishu() {
    await runFeishuAction(async () => {
      const result = await syncFeishuBridge('manual');
      if (result.status === 'synced' && (result.pulled_count > 0 || result.deleted_count > 0)) {
        await syncConfiguredStateChange('feishu_bridge_in').catch(() => undefined);
      }
      return `${result.message} 推送 ${result.pushed_count}，拉取 ${result.pulled_count}，删除 ${result.deleted_count}。`;
    });
  }

  async function handleRebuildFeishuTasklists() {
    const confirmed = await confirm({
      confirmLabel: '重建任务清单',
      message: '这会删除飞书端所有「考研专注*」任务清单并按本地清单重新上传任务，飞书日历不会受影响。',
      title: '重建飞书任务清单？',
      tone: 'danger',
    });
    if (!confirmed) return;

    await runFeishuAction(async () => {
      const result = await rebuildFeishuTasklistsFromLocal();
      return `${result.message} 本地备份：${result.backup_path}；飞书备份：${result.remote_backup_path}`;
    });
  }

  async function handleOpenFeishuLogin() {
    const url = feishuStatus?.pending_authorization_url;
    if (!url) return;
    try {
      await openExternalUrl(url);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="page-shell settings-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">System Console</p>
          <h2>节奏与数据控制</h2>
          <p>默认参数会用于下一次学习模式；学习运行时所有配置入口保持锁定。</p>
        </div>
        <button className="secondary-action" onClick={() => void initializeSettingsPage()} type="button">
          <RefreshCw size={17} />
          刷新
        </button>
      </header>

      {error && <p className="alert error">{error}</p>}
      {savedMessage && <p className="alert success">{savedMessage}</p>}
      {settingsLocked && <p className="alert neutral">学习模式正在运行，全部配置改动已锁定；当前页面只允许查看状态。</p>}
      {confirmDialog}

      <nav className="settings-section-tabs" role="tablist" aria-label="设置分区">
        {settingsSections.map((section) => {
          const Icon = section.icon;
          const selected = activeSection === section.key;

          return (
            <button
              aria-controls={`settings-panel-${section.key}`}
              aria-selected={selected}
              id={`settings-tab-${section.key}`}
              key={section.key}
              onClick={() => setActiveSection(section.key)}
              role="tab"
              type="button"
            >
              <Icon size={16} />
              <span>
                <strong>{section.label}</strong>
                <small>{section.description}</small>
              </span>
            </button>
          );
        })}
      </nav>

      {activeSection === 'basic' && (
        <BasicSettingsPanel
          currentReminderSoundOption={currentReminderSoundOption}
          currentReminderSoundSourceOption={currentReminderSoundSourceOption}
          customReminderSoundFile={customReminderSoundFile}
          customReminderSoundInputKey={customReminderSoundInputKey}
          handlePreviewReminderSound={handlePreviewReminderSound}
          handleReminderSoundFileChange={handleReminderSoundFileChange}
          handleReminderSoundSourceChange={handleReminderSoundSourceChange}
          handleResetReminderSound={handleResetReminderSound}
          handleSaveSettings={handleSaveSettings}
          handleUploadReminderSound={handleUploadReminderSound}
          onThemeChange={onThemeChange}
          reminderSoundActionDisabled={reminderSoundActionDisabled}
          reminderSoundBusy={reminderSoundBusy}
          reminderSoundMessage={reminderSoundMessage}
          reminderSoundOptions={reminderSoundOptions}
          reminderSoundSourceOptions={reminderSoundSourceOptions}
          reminderSoundVolumeStyle={reminderSoundVolumeStyle}
          saving={saving}
          settings={settings}
          settingsLocked={settingsLocked}
          theme={theme}
          updateReminderSoundSettings={updateReminderSoundSettings}
          updateSettings={updateSettings}
        />
      )}

      {activeSection === 'sync' && (
        <SyncSettingsPanel
          expandedPanels={expandedPanels}
          handleDownloadObjectStorage={handleDownloadObjectStorage}
          handleDownloadWebDav={handleDownloadWebDav}
          handlePreviewBackup={handlePreviewBackup}
          handleRestoreBackup={handleRestoreBackup}
          handleSaveObjectStorageSettings={handleSaveObjectStorageSettings}
          handleSaveWebDavSettings={handleSaveWebDavSettings}
          handleTestObjectStorage={handleTestObjectStorage}
          handleTestWebDav={handleTestWebDav}
          handleUploadObjectStorage={handleUploadObjectStorage}
          handleUploadWebDav={handleUploadWebDav}
          lastAutoSyncMessage={lastAutoSyncMessage}
          objectStorageActionDisabled={objectStorageActionDisabled}
          objectStorageBusy={objectStorageBusy}
          objectStorageMessage={objectStorageMessage}
          objectStorageSettings={objectStorageSettings}
          refreshSyncDetails={refreshSyncDetails}
          settings={settings}
          settingsLocked={settingsLocked}
          syncBackups={syncBackups}
          syncDetailMessage={syncDetailMessage}
          syncRuns={syncRuns}
          togglePanel={togglePanel}
          updateObjectStorageSettings={updateObjectStorageSettings}
          updateSyncBackend={updateSyncBackend}
          updateWebDavSettings={updateWebDavSettings}
          webDavActionDisabled={webDavActionDisabled}
          webDavBusy={webDavBusy}
          webDavMessage={webDavMessage}
          webDavSettings={webDavSettings}
        />
      )}

      {activeSection === 'integrations' && (
        <IntegrationsPanel
          emailActionDisabled={emailActionDisabled}
          emailBusy={emailBusy}
          emailMessage={emailMessage}
          emailSettings={emailSettings}
          expandedPanels={expandedPanels}
          feishuActionDisabled={feishuActionDisabled}
          feishuBusy={feishuBusy}
          feishuMessage={feishuMessage}
          feishuSettings={feishuSettings}
          feishuStatus={feishuStatus}
          handleLogoutFeishu={handleLogoutFeishu}
          handleOpenFeishuLogin={handleOpenFeishuLogin}
          handlePollFeishuLogin={handlePollFeishuLogin}
          handleRebuildFeishuTasklists={handleRebuildFeishuTasklists}
          handleSaveEmailSettings={handleSaveEmailSettings}
          handleSaveFeishuSettings={handleSaveFeishuSettings}
          handleStartFeishuLogin={handleStartFeishuLogin}
          handleSyncFeishu={handleSyncFeishu}
          handleTestEmail={handleTestEmail}
          settingsLocked={settingsLocked}
          togglePanel={togglePanel}
          updateEmailSettings={updateEmailSettings}
          updateFeishuSettings={updateFeishuSettings}
        />
      )}

      {activeSection === 'system' && (
        <SystemPanel
          availableUpdate={availableUpdate}
          autoUpdateMessage={lastAutoUpdateMessage}
          checkingUpdate={checkingUpdate}
          dataLocation={dataLocation}
          expandedPanels={expandedPanels}
          foregroundApp={foregroundApp}
          handleCheckUpdate={handleCheckUpdate}
          handleDetectForegroundApp={handleDetectForegroundApp}
          handleInstallUpdate={handleInstallUpdate}
          installingUpdate={installingUpdate}
          loading={loading}
          settingsLocked={settingsLocked}
          togglePanel={togglePanel}
          updateMessage={visibleUpdateMessage}
          updateProgress={updateProgress}
        />
      )}
    </section>
  );
}

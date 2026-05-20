import { useEffect, useState } from 'react';
import {
  Activity,
  BellRing,
  ChevronDown,
  Cloud,
  Database,
  Download,
  ExternalLink,
  Mail,
  HardDrive,
  MonitorDot,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  ShieldCheck,
  UploadCloud,
} from 'lucide-react';
import { getStudyModeState } from '../services/focusApi';
import { getCurrentForegroundApp } from '../services/monitorApi';
import { isStudyModeLocked } from '../services/studyModeLock';
import {
  downloadDatabaseFromWebDav,
  getAppDataLocation,
  getAppSettings,
  getEmailReminderSettings,
  getFeishuSyncSettings,
  getFeishuSyncStatus,
  getObjectStorageSettings,
  getWebDavSettings,
  saveAppSettings,
  saveEmailReminderSettings,
  saveObjectStorageSettings,
  saveWebDavSettings,
  testObjectStorageConnection,
  testWebDavConnection,
  downloadDatabaseFromObjectStorage,
  listSyncBackups,
  listSyncRuns,
  previewSyncBackup,
  pollFeishuOAuthLogin,
  rebuildFeishuTasklistsFromLocal,
  restoreSyncBackup,
  testEmailReminder,
  uploadDatabaseToWebDav,
  uploadDatabaseToObjectStorage,
  logoutFeishu,
  saveFeishuSyncSettings,
  startFeishuOAuthLogin,
  syncConfiguredStateChange,
  syncFeishuBridge,
} from '../services/settingsApi';
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
  sync_backend: 'webdav',
  emergency_cooldown_seconds: 60,
  checklist_category_names: '{"politics":"政治","english":"英语","math":"数学","major":"专业课","general":"通用"}',
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

type SettingsPageProps = {
  lastAutoSyncMessage?: string | null;
  theme?: AppTheme;
  onThemeChange?: (theme: AppTheme) => void;
};

type SettingsPanelKey =
  | 'webdav'
  | 'feishu'
  | 'email'
  | 'syncJournal'
  | 'backups'
  | 'objectStorage'
  | 'rules'
  | 'update'
  | 'foreground';

export default function SettingsPage({ lastAutoSyncMessage = null, theme = 'dark', onThemeChange = () => {} }: SettingsPageProps) {
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
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
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
    if (!window.confirm(`确认恢复备份「${entry.label}」吗？恢复前会先备份当前本地数据库。`)) return;
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
    if (!window.confirm('确认按本地清单重建飞书任务清单吗？这会删除飞书端所有「考研专注*」任务清单并重新上传本地任务，飞书日历不会受影响。')) return;
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

      <section className="command-panel rhythm-section">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Rhythm</p>
            <h3>学习节奏</h3>
          </div>
          <Settings2 size={20} />
        </div>

        <div className="rhythm-grid">
          <SettingNumber label="学习模式时长" max={720} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ default_study_minutes: value })} text="进入学习模式后的总约束时间。" value={settings.default_study_minutes} />
          <SettingNumber label="番茄专注时长" max={120} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ default_focus_minutes: value })} text="学习模式内每轮番茄钟的专注分钟数。" value={settings.default_focus_minutes} />
          <SettingNumber label="短休" max={60} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ break_minutes: value })} text="普通番茄轮次结束后的休息分钟数。" value={settings.break_minutes} />
          <SettingNumber label="长休" max={120} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ long_break_minutes: value })} text="达到长休息轮次后的休息分钟数。" value={settings.long_break_minutes} />
          <SettingNumber label="长休间隔" max={12} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ long_break_interval: value })} text="每几个番茄钟进入一次长休息。" value={settings.long_break_interval} unit="轮" />

          <div className="setting-row mode-setting">
            <div>
              <strong>默认专注模式</strong>
              <p>普通模式更轻量，强制模式会保持更严格的学习约束。</p>
            </div>
            <div className="segmented-control">
              <button className={settings.default_focus_mode === 'normal' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ default_focus_mode: 'normal' })} type="button">普通</button>
              <button className={settings.default_focus_mode === 'strict' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ default_focus_mode: 'strict' })} type="button">强制</button>
            </div>
          </div>

          <div className="setting-row mode-setting">
            <div>
              <strong>界面配色</strong>
              <p>黑色保留当前暗色界面，白色切换为磨砂玻璃风格。</p>
            </div>
            <div className="segmented-control">
              <button className={theme === 'dark' ? 'active' : ''} disabled={settingsLocked} onClick={() => { onThemeChange('dark'); updateSettings({ ui_theme: 'dark' }); }} type="button">黑色</button>
              <button className={theme === 'light' ? 'active' : ''} disabled={settingsLocked} onClick={() => { onThemeChange('light'); updateSettings({ ui_theme: 'light' }); }} type="button">白色磨砂</button>
            </div>
          </div>

          <div className="setting-row mode-setting">
            <div>
              <strong>云同步方式</strong>
              <p>WebDAV 保持默认；对象存储用于 Cloudflare R2、MinIO 或其他 S3 兼容服务。</p>
            </div>
            <div className="segmented-control">
              <button className={settings.sync_backend === 'webdav' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSyncBackend('webdav')} type="button">WebDAV</button>
              <button className={settings.sync_backend === 'object_storage' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSyncBackend('object_storage')} type="button">对象存储</button>
            </div>
          </div>
        </div>

        <div className="settings-save-row">
          <div className="settings-save-copy">
            <span>当前默认节奏</span>
            <strong>
              <b>{settings.default_focus_minutes}</b>
              <small>专注</small>
              <b>{settings.break_minutes}</b>
              <small>短休</small>
              <b>{settings.long_break_minutes}</b>
              <small>长休</small>
              <b>{settings.long_break_interval}</b>
              <small>轮一次</small>
            </strong>
          </div>
          <button className="primary-action" disabled={saving || settingsLocked} onClick={() => void handleSaveSettings()} type="button">
            <Save size={18} />
            {saving ? '保存中' : '保存设置'}
          </button>
        </div>
      </section>

      <div className="settings-grid">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">WebDAV</p>
              <h3>数据同步</h3>
            </div>
            <Cloud size={20} />
            <button
              aria-expanded={expandedPanels.webdav}
              className="settings-collapse-button"
              onClick={() => togglePanel('webdav')}
              type="button"
            >
              <span>{webDavSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.webdav && (
            <>
          <p className="panel-copy">填写 WebDAV 地址和账号后，可以把本机 SQLite 数据库上传到云端，或从云端恢复到本机。</p>

          <label className="capability-row sync-toggle-row">
            <Cloud size={17} />
            <input
              checked={webDavSettings.enabled}
              disabled={settingsLocked}
              onChange={(event) => updateWebDavSettings({ enabled: event.target.checked })}
              type="checkbox"
            />
            <span>启用 WebDAV 同步</span>
          </label>

          <div className="form-stack">
            <label className="field-block">
              <span>WebDAV 地址</span>
              <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ url: event.target.value })} placeholder="https://dav.example.com/remote.php/dav/files/me" value={webDavSettings.url} />
            </label>
            <div className="inline-fields">
              <label className="field-block">
                <span>用户名</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ username: event.target.value })} placeholder="用户名" value={webDavSettings.username} />
              </label>
              <label className="field-block">
                <span>密码</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ password: event.target.value })} placeholder="密码或应用密钥" type="password" value={webDavSettings.password} />
              </label>
            </div>
            <label className="field-block">
              <span>远端文件路径</span>
              <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ remote_path: event.target.value })} placeholder="kaoyan-focus/kaoyan-focus.sqlite3" value={webDavSettings.remote_path} />
            </label>
          </div>

          {lastAutoSyncMessage && <p className="alert neutral">启动自动同步：{lastAutoSyncMessage}</p>}
          {!webDavSettings.enabled && <p className="alert neutral">WebDAV 同步已关闭，保存配置后不会参与启动自动同步。</p>}
          {webDavMessage && <p className="alert success">{webDavMessage}</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleSaveWebDavSettings()} type="button"><Save size={17} />保存</button>
            <button className="secondary-action" disabled={webDavActionDisabled} onClick={() => void handleTestWebDav()} type="button"><Cloud size={17} />测试连接</button>
            <button className="primary-action" disabled={webDavActionDisabled} onClick={() => void handleUploadWebDav()} type="button"><UploadCloud size={17} />上传本机数据</button>
            <button className="secondary-action" disabled={webDavActionDisabled} onClick={() => void handleDownloadWebDav()} type="button"><Download size={17} />从云端恢复</button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Feishu</p>
              <h3>飞书任务 / 日历桥接</h3>
            </div>
            <ExternalLink size={20} />
            <button
              aria-expanded={expandedPanels.feishu}
              className="settings-collapse-button"
              onClick={() => togglePanel('feishu')}
              type="button"
            >
              <span>{feishuStatus?.authenticated ? '已登录' : feishuSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.feishu && (
            <>
          <p className="panel-copy">电脑端连接飞书开放平台，把清单同步到飞书任务，把课表同步到飞书日历。iPhone 可直接用飞书官方 App 查看和编辑。</p>

          <label className="capability-row sync-toggle-row">
            <ExternalLink size={17} />
            <input
              checked={feishuSettings.enabled}
              disabled={settingsLocked}
              onChange={(event) => updateFeishuSettings({ enabled: event.target.checked })}
              type="checkbox"
            />
            <span>启用飞书桥接同步</span>
          </label>

          <div className="form-stack">
            <label className="field-block">
              <span>App ID</span>
              <input
                className="text-input"
                disabled={settingsLocked}
                onChange={(event) => updateFeishuSettings({ app_id: event.target.value })}
                placeholder="飞书应用 App ID"
                value={feishuSettings.app_id}
              />
            </label>
            <label className="field-block">
              <span>App Secret</span>
              <input
                className="text-input"
                disabled={settingsLocked}
                onChange={(event) => updateFeishuSettings({ app_secret: event.target.value })}
                placeholder="只保存在本机，不进入 R2 同步"
                type="password"
                value={feishuSettings.app_secret}
              />
            </label>
            <label className="field-block">
              <span>回调地址</span>
              <input
                className="text-input"
                disabled={settingsLocked}
                onChange={(event) => updateFeishuSettings({ redirect_uri: event.target.value })}
                placeholder="http://127.0.0.1:39781/feishu/callback"
                value={feishuSettings.redirect_uri}
              />
            </label>
            <div className="details-card stacked">
              <Detail label="登录状态" value={feishuStatus?.authenticated ? '已登录' : '未登录'} />
              <Detail label="飞书任务清单" value={feishuStatus?.tasklist_count ? `${feishuStatus.tasklist_count}/6 个分类清单` : '未创建'} />
              <Detail label="飞书日历" value={feishuStatus?.calendar_id ? '考研专注' : '未创建'} />
              <Detail label="回调地址" value={feishuStatus?.redirect_uri ?? feishuSettings.redirect_uri} />
              <Detail label="需要权限" value={feishuStatus?.required_scopes ?? '读取中'} />
              {feishuStatus?.last_run && (
                <Detail label="最近同步" value={`${feishuStatus.last_run.status} · ${feishuStatus.last_run.finished_at}`} />
              )}
            </div>
            {feishuStatus?.tasklists && feishuStatus.tasklists.length > 0 && (
              <div className="details-card stacked">
                {feishuStatus.tasklists.map((tasklist) => (
                  <Detail
                    key={tasklist.key}
                    label={tasklist.label}
                    value={tasklist.ready ? '已创建' : '未创建'}
                  />
                ))}
              </div>
            )}
            {feishuStatus?.pending_authorization_url && (
              <div className="details-card stacked">
                <Detail label="授权页" value={feishuStatus.pending_authorization_url} />
                {feishuStatus.pending_message && <Detail label="提示" value={feishuStatus.pending_message} />}
              </div>
            )}
          </div>

          {feishuMessage && <p className="alert success">{feishuMessage}</p>}
          {!feishuSettings.enabled && <p className="alert neutral">飞书桥接已关闭，自动同步会静默跳过。</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={feishuBusy || settingsLocked} onClick={() => void handleSaveFeishuSettings()} type="button"><Save size={17} />保存</button>
            <button className="secondary-action" disabled={feishuBusy || settingsLocked || !feishuSettings.app_id || !feishuSettings.app_secret} onClick={() => void handleStartFeishuLogin()} type="button"><ExternalLink size={17} />浏览器授权</button>
            <button className="secondary-action" disabled={feishuBusy || !feishuStatus?.pending_authorization_url} onClick={() => void handleOpenFeishuLogin()} type="button"><ExternalLink size={17} />打开授权页</button>
            <button className="secondary-action" disabled={feishuBusy} onClick={() => void handlePollFeishuLogin()} type="button"><RefreshCw size={17} />检查登录</button>
            <button className="primary-action" disabled={feishuActionDisabled || !feishuStatus?.authenticated} onClick={() => void handleSyncFeishu()} type="button"><RefreshCw size={17} />立即同步</button>
            <button className="secondary-action danger-action" disabled={feishuActionDisabled || !feishuStatus?.authenticated} onClick={() => void handleRebuildFeishuTasklists()} type="button"><RefreshCw size={17} />重建任务清单</button>
            <button className="secondary-action" disabled={feishuBusy || settingsLocked || !feishuStatus?.authenticated} onClick={() => void handleLogoutFeishu()} type="button">退出登录</button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">SMTP</p>
              <h3>截止任务邮件提醒</h3>
            </div>
            <Mail size={20} />
            <button
              aria-expanded={expandedPanels.email}
              className="settings-collapse-button"
              onClick={() => togglePanel('email')}
              type="button"
            >
              <span>{emailSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.email && (
            <>
          <p className="panel-copy">电脑端在每天 21:00 检查明天到期且未完成的清单/今日任务，并只发送一次邮件。SMTP 密码只保存在本机。</p>

          <label className="capability-row sync-toggle-row">
            <Mail size={17} />
            <input
              checked={emailSettings.enabled}
              disabled={settingsLocked}
              onChange={(event) => updateEmailSettings({ enabled: event.target.checked })}
              type="checkbox"
            />
            <span>启用邮件提醒</span>
          </label>

          <div className="form-stack">
            <div className="inline-fields">
              <label className="field-block">
                <span>SMTP Host</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ smtp_host: event.target.value })} placeholder="smtp.example.com" value={emailSettings.smtp_host} />
              </label>
              <label className="field-block">
                <span>Port</span>
                <input className="text-input" disabled={settingsLocked} min={1} max={65535} onChange={(event) => updateEmailSettings({ smtp_port: Number(event.target.value) || 465 })} type="number" value={emailSettings.smtp_port} />
              </label>
            </div>
            <div className="inline-fields">
              <label className="field-block">
                <span>加密方式</span>
                <select className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ smtp_security: event.target.value as EmailReminderSettings['smtp_security'] })} value={emailSettings.smtp_security}>
                  <option value="tls">TLS / SSL</option>
                  <option value="starttls">STARTTLS</option>
                  <option value="none">不加密</option>
                </select>
              </label>
              <label className="field-block">
                <span>账号</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ username: event.target.value })} placeholder="邮箱账号 / SMTP 用户名" value={emailSettings.username} />
              </label>
            </div>
            <label className="field-block">
              <span>授权码 / 密码</span>
              <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ password: event.target.value })} placeholder="只保存在本机设置" type="password" value={emailSettings.password} />
            </label>
            <div className="inline-fields">
              <label className="field-block">
                <span>发件人</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ from: event.target.value })} placeholder="me@example.com" value={emailSettings.from} />
              </label>
              <label className="field-block">
                <span>收件人</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ to: event.target.value })} placeholder="target@example.com" value={emailSettings.to} />
              </label>
            </div>
          </div>

          {emailMessage && <p className="alert success">{emailMessage}</p>}
          {!emailSettings.enabled && <p className="alert neutral">邮件提醒已关闭，后台检查会静默跳过。</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={emailBusy || settingsLocked} onClick={() => void handleSaveEmailSettings()} type="button"><Save size={17} />保存</button>
            <button className="secondary-action" disabled={emailActionDisabled} onClick={() => void handleTestEmail()} type="button"><Mail size={17} />测试发送</button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Sync Journal</p>
              <h3>最近同步详情</h3>
            </div>
            <Activity size={20} />
            <button
              aria-expanded={expandedPanels.syncJournal}
              className="settings-collapse-button"
              onClick={() => togglePanel('syncJournal')}
              type="button"
            >
              <span>{syncRuns[0]?.status ?? '暂无记录'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.syncJournal && (
            <>
          {syncDetailMessage && <p className="alert neutral">{syncDetailMessage}</p>}
          <div className="settings-list">
            {syncRuns.length === 0 ? (
              <p className="panel-copy">暂无同步日志。完成一次对象存储同步后会显示校验、备份和接管信息。</p>
            ) : syncRuns.map((run) => (
              <div className="details-card stacked" key={run.id}>
                <Detail label={`${run.trigger} · ${run.status}`} value={run.finished_at} />
                <Detail label="方向 / 耗时" value={`${run.direction ?? 'skip'} · ${run.duration_ms}ms`} />
                <Detail label="实体 / 删除 / 字节" value={`${run.exported_count} / ${run.deleted_count} / ${formatBytes(run.bytes)}`} />
                <Detail label="Active / Remote" value={`${run.active_snapshot_sync_id ?? 'none'} (${run.active_snapshot_phase ?? '-'}) / ${run.remote_active_snapshot_sync_id ?? 'none'} (${run.remote_active_snapshot_phase ?? '-'})`} />
                <Detail label="UpdatedAt" value={`${run.active_snapshot_updated_at ?? '-'} / ${run.remote_snapshot_updated_at ?? '-'}`} />
                {run.remote_exported_drift_seconds !== null && <Detail label="Clock drift" value={`${run.remote_exported_drift_seconds}s`} />}
                {run.detail && <Detail label="Decision" value={run.detail} />}
                {run.validation_report && <Detail label="校验" value={run.validation_report} />}
                {run.backup_path && <Detail label="本地备份" value={run.backup_path} />}
                {run.remote_backup_key && <Detail label="云端备份" value={run.remote_backup_key} />}
              </div>
            ))}
          </div>
          <div className="row-actions">
            <button className="secondary-action" disabled={objectStorageBusy} onClick={() => void refreshSyncDetails()} type="button"><RefreshCw size={17} />刷新详情</button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Restore</p>
              <h3>备份恢复</h3>
            </div>
            <RotateCcw size={20} />
            <button
              aria-expanded={expandedPanels.backups}
              className="settings-collapse-button"
              onClick={() => togglePanel('backups')}
              type="button"
            >
              <span>{syncBackups.length} 个备份</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.backups && (
            <>
          <p className="panel-copy">恢复前先点预检查看校验结果；真正恢复会自动备份当前本地数据库。</p>
          <div className="settings-list">
            {syncBackups.length === 0 ? (
              <p className="panel-copy">暂无可用备份。</p>
            ) : syncBackups.slice(0, 8).map((entry) => (
              <div className="details-card stacked" key={`${entry.source}:${entry.key}`}>
                <Detail label={`${entry.source.toUpperCase()} · ${entry.label}`} value={entry.created_at ?? '未知时间'} />
                <Detail label="大小" value={entry.bytes === null ? '未知' : formatBytes(entry.bytes)} />
                <div className="row-actions">
                  <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handlePreviewBackup(entry)} type="button">预检</button>
                  <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handleRestoreBackup(entry)} type="button">恢复</button>
                </div>
              </div>
            ))}
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">S3 / R2</p>
              <h3>对象存储同步</h3>
            </div>
            <Cloud size={20} />
            <button
              aria-expanded={expandedPanels.objectStorage}
              className="settings-collapse-button"
              onClick={() => togglePanel('objectStorage')}
              type="button"
            >
              <span>{objectStorageSettings.enabled ? objectStorageSettings.object_key || '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.objectStorage && (
            <>
          <p className="panel-copy">填写 Cloudflare R2 或 S3 兼容对象存储信息后，应用会使用共享 JSON 数据包与手机端同步。</p>

          <label className="capability-row sync-toggle-row">
            <Cloud size={17} />
            <input
              checked={objectStorageSettings.enabled}
              disabled={settingsLocked}
              onChange={(event) => updateObjectStorageSettings({ enabled: event.target.checked })}
              type="checkbox"
            />
            <span>启用对象存储同步</span>
          </label>

          <div className="form-stack">
            <label className="field-block">
              <span>Endpoint</span>
              <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ endpoint: event.target.value })} placeholder="https://<account-id>.r2.cloudflarestorage.com" value={objectStorageSettings.endpoint} />
            </label>
            <div className="inline-fields">
              <label className="field-block">
                <span>Bucket</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ bucket: event.target.value })} placeholder="kaoyan-focus" value={objectStorageSettings.bucket} />
              </label>
              <label className="field-block">
                <span>Region</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ region: event.target.value })} placeholder="auto" value={objectStorageSettings.region} />
              </label>
            </div>
            <div className="inline-fields">
              <label className="field-block">
                <span>Access Key ID</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ access_key_id: event.target.value })} placeholder="由对象存储控制台生成" value={objectStorageSettings.access_key_id} />
              </label>
              <label className="field-block">
                <span>Secret Access Key</span>
                <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ secret_access_key: event.target.value })} placeholder="只保存在本机数据" type="password" value={objectStorageSettings.secret_access_key} />
              </label>
            </div>
            <label className="field-block">
              <span>Object Key</span>
              <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ object_key: event.target.value })} placeholder="study-sync.json" value={objectStorageSettings.object_key} />
            </label>
          </div>

          {objectStorageMessage && <p className="alert success">{objectStorageMessage}</p>}
          {!objectStorageSettings.enabled && <p className="alert neutral">对象存储同步已关闭，保存配置后不会参与启动自动同步。</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handleSaveObjectStorageSettings()} type="button"><Save size={17} />保存</button>
            <button className="secondary-action" disabled={objectStorageActionDisabled} onClick={() => void handleTestObjectStorage()} type="button"><Cloud size={17} />测试连接</button>
            <button className="primary-action" disabled={objectStorageActionDisabled} onClick={() => void handleUploadObjectStorage()} type="button"><UploadCloud size={17} />上传本机数据</button>
            <button className="secondary-action" disabled={objectStorageActionDisabled} onClick={() => void handleDownloadObjectStorage()} type="button"><Download size={17} />从云端恢复</button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Rules</p>
              <h3>强制规则</h3>
            </div>
            <HardDrive size={20} />
            <button
              aria-expanded={expandedPanels.rules}
              className="settings-collapse-button"
              onClick={() => togglePanel('rules')}
              type="button"
            >
              <span>{settingsLocked ? '运行中' : '查看'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.rules && (
            <>

          <div className="settings-list">
            <Capability enabled={settingsLocked} icon={BellRing} text="学习期间关闭窗口时最小化到托盘" />
            <Capability enabled icon={ShieldCheck} text="记录非白名单应用干扰事件" />
            <Capability enabled={Boolean(dataLocation)} icon={Database} text="SQLite 本地数据目录可用" />
          </div>

          {dataLocation && (
            <div className="details-card">
              <span>数据文件路径</span>
              <strong>{dataLocation}</strong>
            </div>
          )}
            </>
          )}
        </section>
      </div>

      <div className="settings-grid lower">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Update</p>
              <h3>在线更新</h3>
            </div>
            <Download size={20} />
            <button
              aria-expanded={expandedPanels.update}
              className="settings-collapse-button"
              onClick={() => togglePanel('update')}
              type="button"
            >
              <span>{availableUpdate ? availableUpdate.version : '手动检查'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.update && (
            <>
          <p className="panel-copy">检查发布服务器上的新版本，下载完成后会自动重启应用。</p>
          {updateMessage && <p className="alert neutral">{updateMessage}</p>}
          {updateProgress !== null && <p className="alert neutral">下载进度 {updateProgress}%</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={checkingUpdate || installingUpdate || settingsLocked} onClick={() => void handleCheckUpdate()} type="button">
              <RefreshCw size={17} />
              {checkingUpdate ? '检查中' : '检查更新'}
            </button>
            <button className="primary-action" disabled={availableUpdate === null || installingUpdate || settingsLocked} onClick={() => void handleInstallUpdate()} type="button">
              <Download size={17} />
              {installingUpdate ? '安装中' : '下载并安装'}
            </button>
          </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Foreground</p>
              <h3>前台应用检测</h3>
            </div>
            <MonitorDot size={20} />
            <button
              aria-expanded={expandedPanels.foreground}
              className="settings-collapse-button"
              onClick={() => togglePanel('foreground')}
              type="button"
            >
              <span>{foregroundApp?.process_name ?? '诊断'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.foreground && (
            <>
          <p className="panel-copy">用于验证 Windows API 能否识别当前正在使用的窗口和进程。</p>
          <button className="secondary-action" disabled={loading} onClick={() => void handleDetectForegroundApp()} type="button">
            <Activity size={17} />
            {loading ? '检测中' : '检测当前应用'}
          </button>

          {foregroundApp && (
            <div className="details-card stacked">
              <Detail label="进程名" value={foregroundApp.process_name} />
              <Detail label="进程 ID" value={String(foregroundApp.process_id)} />
              <Detail label="窗口标题" value={foregroundApp.window_title || '无标题'} />
              <Detail label="进程路径" value={foregroundApp.process_path || '无法读取'} />
            </div>
          )}
            </>
          )}
        </section>
      </div>
    </section>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function SettingNumber({
  disabled,
  label,
  max,
  min,
  onChange,
  text,
  unit = '分钟',
  value,
}: {
  disabled: boolean;
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  text: string;
  unit?: string;
  value: number;
}) {
  function step(delta: number) {
    onChange(Math.min(max, Math.max(min, value + delta)));
  }

  return (
    <div className="setting-row rhythm-card">
      <div>
        <strong>{label}</strong>
        <p>{text}</p>
      </div>
      <div className="stepper-control">
        <button aria-label={`${label}减少`} disabled={disabled || value <= min} onClick={() => step(-1)} type="button">-</button>
        <label>
          <input
            className="number-input"
            disabled={disabled}
            max={max}
            min={min}
            onChange={(event) => onChange(Math.min(max, Math.max(min, Number(event.target.value) || min)))}
            type="number"
            value={value}
          />
          <span>{unit}</span>
        </label>
        <button aria-label={`${label}增加`} disabled={disabled || value >= max} onClick={() => step(1)} type="button">+</button>
      </div>
    </div>
  );
}

function Capability({ enabled, icon: Icon, text }: { enabled: boolean; icon: typeof BellRing; text: string }) {
  return (
    <label className="capability-row">
      <Icon size={17} />
      <input checked={enabled} readOnly type="checkbox" />
      <span>{text}</span>
    </label>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

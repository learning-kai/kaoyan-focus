import { useEffect, useState } from 'react';
import {
  Activity,
  BellRing,
  Cloud,
  Database,
  Download,
  HardDrive,
  MonitorDot,
  RefreshCw,
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
  getWebDavSettings,
  saveAppSettings,
  saveWebDavSettings,
  testWebDavConnection,
  uploadDatabaseToWebDav,
} from '../services/settingsApi';
import { checkForAppUpdate, installAppUpdate, type AppUpdate } from '../services/updateApi';
import type { StudyModeState } from '../types/focus';
import type { ForegroundApp } from '../types/monitor';
import type { AppSettings, WebDavSettings } from '../types/settings';

const defaultSettings: AppSettings = {
  default_study_minutes: 120,
  default_focus_minutes: 25,
  break_minutes: 5,
  long_break_minutes: 15,
  long_break_interval: 4,
  default_focus_mode: 'normal',
  emergency_cooldown_seconds: 60,
};

const defaultWebDavSettings: WebDavSettings = {
  url: '',
  username: '',
  password: '',
  remote_path: 'kaoyan-focus/kaoyan-focus.sqlite3',
};

type SettingsPageProps = {
  lastAutoSyncMessage?: string | null;
};

export default function SettingsPage({ lastAutoSyncMessage = null }: SettingsPageProps) {
  const [foregroundApp, setForegroundApp] = useState<ForegroundApp | null>(null);
  const [studyState, setStudyState] = useState<StudyModeState | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [webDavSettings, setWebDavSettings] = useState<WebDavSettings>(defaultWebDavSettings);
  const [dataLocation, setDataLocation] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const [webDavMessage, setWebDavMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AppUpdate | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [webDavBusy, setWebDavBusy] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);

  const settingsLocked = isStudyModeLocked(studyState);

  useEffect(() => {
    void initializeSettingsPage();
  }, []);

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
    await Promise.all([refreshStudyState(), refreshSettings(), refreshDataLocation(), refreshWebDavSettings()]);
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
      setSettings(await getAppSettings());
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

  async function refreshDataLocation() {
    try {
      setError(null);
      const location = await getAppDataLocation();
      setDataLocation(location.database_path);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleSaveSettings() {
    try {
      setSaving(true);
      setError(null);
      setSavedMessage(null);
      const saved = await saveAppSettings(settings);
      setSettings(saved);
      setSavedMessage('设置已保存，下次进入专注页会自动使用。');
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

  function updateSettings(patch: Partial<AppSettings>) {
    setSavedMessage(null);
    setSettings((current) => ({ ...current, ...patch }));
  }

  function updateWebDavSettings(patch: Partial<WebDavSettings>) {
    setWebDavMessage(null);
    setWebDavSettings((current) => ({ ...current, ...patch }));
  }

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

      setUpdateMessage(`发现新版本 ${update.version}${update.body ? `：${update.body}` : ''}`);
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
          <SettingNumber
            label="学习模式时长"
            max={720}
            min={1}
            disabled={settingsLocked}
            onChange={(value) => updateSettings({ default_study_minutes: value })}
            text="进入学习模式后的总约束时间。"
            value={settings.default_study_minutes}
          />
          <SettingNumber
            label="番茄专注时长"
            max={120}
            min={1}
            disabled={settingsLocked}
            onChange={(value) => updateSettings({ default_focus_minutes: value })}
            text="学习模式内每轮番茄钟的专注分钟数。"
            value={settings.default_focus_minutes}
          />
          <SettingNumber
            label="短休息"
            max={60}
            min={1}
            disabled={settingsLocked}
            onChange={(value) => updateSettings({ break_minutes: value })}
            text="普通番茄轮次结束后的休息分钟数。"
            value={settings.break_minutes}
          />
          <SettingNumber
            label="长休息"
            max={120}
            min={1}
            disabled={settingsLocked}
            onChange={(value) => updateSettings({ long_break_minutes: value })}
            text="到达长休息轮次后的休息分钟数。"
            value={settings.long_break_minutes}
          />
          <SettingNumber
            label="长休间隔"
            max={12}
            min={1}
            disabled={settingsLocked}
            onChange={(value) => updateSettings({ long_break_interval: value })}
            text="每几个番茄钟进入一次长休息。"
            value={settings.long_break_interval}
            unit="轮"
          />

          <div className="setting-row mode-setting">
            <div>
              <strong>默认专注模式</strong>
              <p>普通模式更轻量，强制模式会保持更严格的学习约束。</p>
            </div>
            <div className="segmented-control">
              <button
                className={settings.default_focus_mode === 'normal' ? 'active' : ''}
                disabled={settingsLocked}
                onClick={() => updateSettings({ default_focus_mode: 'normal' })}
                type="button"
              >
                普通
              </button>
              <button
                className={settings.default_focus_mode === 'strict' ? 'active' : ''}
                disabled={settingsLocked}
                onClick={() => updateSettings({ default_focus_mode: 'strict' })}
                type="button"
              >
                强制
              </button>
            </div>
          </div>
        </div>

        <div className="settings-save-row">
          <div>
            <span>当前默认节奏</span>
            <strong>{settings.default_focus_minutes} / {settings.break_minutes} / {settings.long_break_minutes} 分钟，每 {settings.long_break_interval} 轮长休</strong>
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
          </div>
          <p className="panel-copy">填写 WebDAV 地址和账号后，可以把本机 SQLite 数据库上传到云端，或从云端恢复到本机。</p>

          <div className="form-stack">
            <label className="field-block">
              <span>WebDAV 地址</span>
              <input
                className="text-input"
                disabled={settingsLocked}
                onChange={(event) => updateWebDavSettings({ url: event.target.value })}
                placeholder="https://dav.example.com/remote.php/dav/files/me"
                value={webDavSettings.url}
              />
            </label>
            <div className="inline-fields">
              <label className="field-block">
                <span>用户名</span>
                <input
                  className="text-input"
                  disabled={settingsLocked}
                  onChange={(event) => updateWebDavSettings({ username: event.target.value })}
                  placeholder="用户名"
                  value={webDavSettings.username}
                />
              </label>
              <label className="field-block">
                <span>密码</span>
                <input
                  className="text-input"
                  disabled={settingsLocked}
                  onChange={(event) => updateWebDavSettings({ password: event.target.value })}
                  placeholder="密码或应用密码"
                  type="password"
                  value={webDavSettings.password}
                />
              </label>
            </div>
            <label className="field-block">
              <span>远端文件路径</span>
              <input
                className="text-input"
                disabled={settingsLocked}
                onChange={(event) => updateWebDavSettings({ remote_path: event.target.value })}
                placeholder="kaoyan-focus/kaoyan-focus.sqlite3"
                value={webDavSettings.remote_path}
              />
            </label>
          </div>

          {lastAutoSyncMessage && <p className="alert neutral">启动自动同步：{lastAutoSyncMessage}</p>}
          {webDavMessage && <p className="alert success">{webDavMessage}</p>}
          <div className="row-actions">
            <button className="secondary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleSaveWebDavSettings()} type="button">
              <Save size={17} />
              保存
            </button>
            <button className="secondary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleTestWebDav()} type="button">
              <Cloud size={17} />
              测试连接
            </button>
            <button className="primary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleUploadWebDav()} type="button">
              <UploadCloud size={17} />
              上传本机数据
            </button>
            <button className="secondary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleDownloadWebDav()} type="button">
              <Download size={17} />
              从云端恢复
            </button>
          </div>
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Rules</p>
              <h3>强制规则</h3>
            </div>
            <HardDrive size={20} />
          </div>

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
          </div>
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
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Foreground</p>
              <h3>前台应用检测</h3>
            </div>
            <MonitorDot size={20} />
          </div>
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

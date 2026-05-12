import { useEffect, useState } from 'react';
import {
  Activity,
  BellRing,
  Database,
  Download,
  HardDrive,
  MonitorDot,
  RefreshCw,
  Save,
  Settings2,
  ShieldCheck,
} from 'lucide-react';
import { listFocusSessions } from '../services/focusApi';
import { getCurrentForegroundApp } from '../services/monitorApi';
import { getAppDataLocation, getAppSettings, saveAppSettings } from '../services/settingsApi';
import { checkForAppUpdate, installAppUpdate, type AppUpdate } from '../services/updateApi';
import type { FocusSession } from '../types/focus';
import type { ForegroundApp } from '../types/monitor';
import type { AppSettings } from '../types/settings';

const defaultSettings: AppSettings = {
  default_study_minutes: 120,
  default_focus_minutes: 25,
  break_minutes: 5,
  default_focus_mode: 'normal',
  emergency_cooldown_seconds: 60,
};

export default function SettingsPage() {
  const [foregroundApp, setForegroundApp] = useState<ForegroundApp | null>(null);
  const [latestSession, setLatestSession] = useState<FocusSession | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [dataLocation, setDataLocation] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AppUpdate | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);

  useEffect(() => {
    void initializeSettingsPage();
  }, []);

  async function initializeSettingsPage() {
    await Promise.all([refreshFocusState(), refreshSettings(), refreshDataLocation()]);
  }

  async function refreshFocusState() {
    try {
      const sessions = await listFocusSessions();
      setLatestSession(sessions[0] ?? null);
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

  function updateSettings(patch: Partial<AppSettings>) {
    setSavedMessage(null);
    setSettings((current) => ({ ...current, ...patch }));
  }

  async function handleDetectForegroundApp() {
    try {
      setLoading(true);
      setError(null);
      setForegroundApp(await getCurrentForegroundApp());
      await refreshFocusState();
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

  const focusRunning = latestSession?.status === 'running';

  return (
    <section className="page-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">设置 / 本地持久化</p>
          <h2>应用设置</h2>
          <p>配置默认学习参数、更新渠道和 Windows 前台检测能力。设置会保存到本地 SQLite 数据库。</p>
        </div>
        <button className="secondary-action" onClick={() => void initializeSettingsPage()} type="button">
          <RefreshCw size={17} />
          刷新
        </button>
      </header>

      {error && <p className="alert error">{error}</p>}
      {savedMessage && <p className="alert success">{savedMessage}</p>}

      <div className="content-grid two">
        <section className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Defaults</p>
              <h3>默认学习参数</h3>
            </div>
            <Settings2 size={20} />
          </div>

          <div className="settings-panel">
            <SettingNumber
              label="学习模式时长"
              max={720}
              min={1}
              onChange={(value) => updateSettings({ default_study_minutes: value })}
              text="进入学习模式后的总约束时间。"
              value={settings.default_study_minutes}
            />
            <SettingNumber
              label="番茄专注时长"
              max={120}
              min={1}
              onChange={(value) => updateSettings({ default_focus_minutes: value })}
              text="学习模式内每轮番茄钟的专注分钟数。"
              value={settings.default_focus_minutes}
            />
            <SettingNumber
              label="休息时长"
              max={60}
              min={1}
              onChange={(value) => updateSettings({ break_minutes: value })}
              text="本人确认开始休息后的倒计时分钟数。"
              value={settings.break_minutes}
            />

            <div className="setting-row">
              <div>
                <strong>默认专注模式</strong>
                <p>普通模式更轻量，强制模式会保持更严格的学习约束。</p>
              </div>
              <div className="segmented-control">
                <button
                  className={settings.default_focus_mode === 'normal' ? 'active' : ''}
                  onClick={() => updateSettings({ default_focus_mode: 'normal' })}
                  type="button"
                >
                  普通
                </button>
                <button
                  className={settings.default_focus_mode === 'strict' ? 'active' : ''}
                  onClick={() => updateSettings({ default_focus_mode: 'strict' })}
                  type="button"
                >
                  强制
                </button>
              </div>
            </div>
          </div>

          <button className="primary-action" disabled={saving} onClick={() => void handleSaveSettings()} type="button">
            <Save size={18} />
            {saving ? '保存中' : '保存设置'}
          </button>
        </section>

        <section className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">System</p>
              <h3>系统状态</h3>
            </div>
            <HardDrive size={20} />
          </div>

          <div className="settings-list">
            <Capability enabled={focusRunning} icon={BellRing} text="专注期间关闭窗口时最小化到托盘" />
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

      <div className="content-grid two">
        <section className="panel">
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
            <button className="secondary-action" disabled={checkingUpdate || installingUpdate} onClick={() => void handleCheckUpdate()} type="button">
              <RefreshCw size={17} />
              {checkingUpdate ? '检查中' : '检查更新'}
            </button>
            <button className="primary-action" disabled={availableUpdate === null || installingUpdate} onClick={() => void handleInstallUpdate()} type="button">
              <Download size={17} />
              {installingUpdate ? '安装中' : '下载并安装'}
            </button>
          </div>
        </section>

        <section className="panel">
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

      {focusRunning && <p className="alert neutral">当前有进行中的专注。点击窗口关闭按钮会隐藏到托盘，可从托盘图标重新打开。</p>}
    </section>
  );
}

function SettingNumber({
  label,
  max,
  min,
  onChange,
  text,
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  text: string;
  value: number;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <p>{text}</p>
      </div>
      <input
        className="number-input"
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.target.value) || min)}
        type="number"
        value={value}
      />
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

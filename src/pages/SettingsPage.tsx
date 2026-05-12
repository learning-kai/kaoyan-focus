import { useEffect, useState } from 'react';
import { listFocusSessions } from '../services/focusApi';
import { getCurrentForegroundApp } from '../services/monitorApi';
import { getAppDataLocation, getAppSettings, saveAppSettings } from '../services/settingsApi';
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
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

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

  const focusRunning = latestSession?.status === 'running';

  return (
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 8 / 设置持久化</p>
        <h2>设置</h2>
        <p>配置默认专注参数和约束边界，所有设置都会保存到本地数据库。</p>
      </div>

      <div className="settings-panel">
        <div className="setting-row">
          <div>
            <strong>默认学习模式时长</strong>
            <p>进入学习模式后的总约束时间。</p>
          </div>
          <input
            className="number-input"
            max={720}
            min={1}
            onChange={(event) => updateSettings({ default_study_minutes: Number(event.target.value) || 1 })}
            type="number"
            value={settings.default_study_minutes}
          />
        </div>

        <div className="setting-row">
          <div>
            <strong>默认番茄专注时长</strong>
            <p>学习模式内每轮番茄钟的专注分钟数。</p>
          </div>
          <input
            className="number-input"
            max={120}
            min={1}
            onChange={(event) => updateSettings({ default_focus_minutes: Number(event.target.value) || 1 })}
            type="number"
            value={settings.default_focus_minutes}
          />
        </div>

        <div className="setting-row">
          <div>
            <strong>默认休息时长</strong>
            <p>本人确认开始休息后的倒计时分钟数。</p>
          </div>
          <input
            className="number-input"
            max={60}
            min={1}
            onChange={(event) => updateSettings({ break_minutes: Number(event.target.value) || 1 })}
            type="number"
            value={settings.break_minutes}
          />
        </div>

        <div className="setting-row">
          <div>
            <strong>默认专注模式</strong>
            <p>普通模式更轻量，严格模式会启用应急退出流程。</p>
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
              严格
            </button>
          </div>
        </div>

        <div className="setting-row">
          <div>
            <strong>应急退出冷静期</strong>
            <p>严格模式下点击应急退出后需要等待的秒数。</p>
          </div>
          <input
            className="number-input"
            max={300}
            min={0}
            onChange={(event) => updateSettings({ emergency_cooldown_seconds: Number(event.target.value) || 0 })}
            type="number"
            value={settings.emergency_cooldown_seconds}
          />
        </div>

        <button className="primary-action" disabled={saving} onClick={() => void handleSaveSettings()} type="button">
          {saving ? '保存中' : '保存设置'}
        </button>
      </div>

      {dataLocation && (
        <div className="details-card">
          <div>
            <span>数据文件路径</span>
            <strong>{dataLocation}</strong>
          </div>
        </div>
      )}

      {savedMessage && <p className="success-text">{savedMessage}</p>}

      <div className="settings-list">
        <label>
          <input checked={focusRunning} readOnly type="checkbox" />
          专注期间关闭窗口时最小化到托盘
        </label>
        <label>
          <input checked readOnly type="checkbox" />
          严格模式应急退出会记录退出次数
        </label>
        <label>
          <input checked readOnly type="checkbox" />
          记录非白名单应用干扰事件
        </label>
      </div>

      <div className="tool-card">
        <div>
          <h3>当前前台应用检测</h3>
          <p>用于验证 Windows API 能否识别当前正在使用的窗口和进程。</p>
        </div>
        <button className="small-action enabled" disabled={loading} onClick={() => void handleDetectForegroundApp()} type="button">
          {loading ? '检测中' : '检测当前应用'}
        </button>
      </div>

      {focusRunning && <p className="notice">当前有进行中的专注。此时点击窗口关闭按钮会隐藏到托盘，可从托盘图标重新打开。</p>}
      {error && <p className="error-text">{error}</p>}

      {foregroundApp && (
        <div className="details-card">
          <div>
            <span>进程名</span>
            <strong>{foregroundApp.process_name}</strong>
          </div>
          <div>
            <span>进程 ID</span>
            <strong>{foregroundApp.process_id}</strong>
          </div>
          <div>
            <span>窗口标题</span>
            <strong>{foregroundApp.window_title || '无标题'}</strong>
          </div>
          <div>
            <span>进程路径</span>
            <strong>{foregroundApp.process_path || '无法读取'}</strong>
          </div>
        </div>
      )}
    </section>
  );
}

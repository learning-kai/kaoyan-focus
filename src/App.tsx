import { type ReactNode, useEffect, useState } from 'react';
import { AlarmClock, BarChart3, CalendarDays, ClipboardList, NotebookPen, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import AlarmPage from './pages/AlarmPage';
import ChecklistPage from './pages/ChecklistPage';
import SchedulePage from './pages/SchedulePage';
import ReviewPage from './pages/ReviewPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
import {
  notifyPersistentAlarm,
  notifyStudyReminder,
  stopPersistentAlarmSound,
} from './services/alertApi';
import {
  ALARM_STATE_CHANGED_EVENT,
  getNextAlarm,
  notifyAlarmStateChanged,
  triggerDueAlarms,
} from './services/alarmApi';
import { getStudyModeState } from './services/focusApi';
import { getSchedulePageData } from './services/scheduleApi';
import {
  autoSyncConfiguredDatabase,
  syncConfiguredStateChange,
  STUDY_SYNC_STATE_CHANGED_EVENT,
} from './services/syncApi';
import {
  FEISHU_SYNC_REFRESH_EVENT,
  syncFeishuBridge,
} from './services/feishuApi';
import { checkDueTaskEmailReminders } from './services/emailApi';
import { getAppSettings, saveAppSettings } from './services/settingsApi';
import type { AppPage } from './types/navigation';
import { applyTheme, bootstrapTheme, storeTheme } from './theme';
import type { Alarm } from './types/alarm';
import type { AppTheme } from './types/settings';
import { listenTauriEvent } from './services/tauriEvents';
import { checkForAppUpdate } from './services/updateApi';
import { showStudyReminder } from './services/systemApi';
import { isTauriRuntime } from './services/tauriInvoke';

const AUTO_SYNC_STARTUP_DELAY_MS = 5000;
const AUTO_SYNC_INTERVAL_MS = 60 * 1000;
const ACTIVE_AUTO_SYNC_INTERVAL_MS = 60 * 1000;
const AUTO_UPDATE_CHECK_STARTUP_DELAY_MS = 12 * 1000;
const AUTO_UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const AUTO_UPDATE_NOTICE_STORAGE_KEY = 'kaoyan-focus:last-auto-update-notice-version';
const FEISHU_AUTO_SYNC_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_LOOKBACK_MINUTES = 1;
const EMAIL_REMINDER_INTERVAL_MS = 5 * 60 * 1000;
const ALARM_CHECK_INTERVAL_MS = 1000;
const SILENT_AUTO_SYNC_SKIP_REASONS = new Set([
  'webdav_not_configured',
  'webdav_disabled',
  'object_storage_not_configured',
  'object_storage_disabled',
  'object_storage_sync_in_flight',
  'study_mode_active',
]);

function todayString() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function currentMinuteOfDay() {
  const now = new Date();
  return now.getHours() * 60 + now.getMinutes();
}

export type PageMeta = {
  title: string;
  description: string;
  icon: LucideIcon;
  component: ReactNode;
};

const pages: Record<AppPage, PageMeta> = {
  focus: {
    title: '专注',
    description: '学习模式与番茄钟',
    icon: TimerReset,
    component: <FocusPage />,
  },
  alarm: {
    title: '闹钟',
    description: '全局一次性提醒',
    icon: AlarmClock,
    component: <AlarmPage />,
  },
  checklist: {
    title: '清单',
    description: '五类待办与今日任务',
    icon: ClipboardList,
    component: <ChecklistPage />,
  },
  schedule: {
    title: '课表',
    description: '今日安排与本周视图',
    icon: CalendarDays,
    component: <SchedulePage />,
  },
  review: {
    title: '复盘',
    description: '每日总结与明日重点',
    icon: NotebookPen,
    component: <ReviewPage />,
  },
  whitelist: {
    title: '白名单',
    description: '软件与网站放行',
    icon: ShieldCheck,
    component: <WhitelistPage />,
  },
  stats: {
    title: '统计',
    description: '学习记录与干扰',
    icon: BarChart3,
    component: <StatsPage />,
  },
  settings: {
    title: '设置',
    description: '节奏、同步与更新',
    icon: Settings,
    component: <SettingsPage />,
  },
};

export default function App() {
  const [activePage, setActivePage] = useState<AppPage>('focus');
  const [lastAutoSyncMessage, setLastAutoSyncMessage] = useState<string | null>(null);
  const [lastAutoUpdateMessage, setLastAutoUpdateMessage] = useState<string | null>(null);
  const [nextAlarm, setNextAlarm] = useState<Alarm | null>(null);
  const [theme, setTheme] = useState<AppTheme>(() => bootstrapTheme());

  useEffect(() => {
    applyTheme(theme);
    storeTheme(theme);
  }, [theme]);

  useEffect(() => {
    let disposed = false;
    let syncInFlight = false;
    let feishuSyncInFlight = false;
    let lastAutoSyncAt = 0;
    let lastFeishuAutoSyncAt = 0;

    async function runFeishuSync(trigger = 'auto') {
      if (feishuSyncInFlight) {
        return;
      }

      lastFeishuAutoSyncAt = Date.now();
      feishuSyncInFlight = true;
      await syncFeishuBridge(trigger)
        .then((result) => {
          if (result.status === 'synced' && (result.pulled_count > 0 || result.deleted_count > 0)) {
            window.dispatchEvent(new CustomEvent(FEISHU_SYNC_REFRESH_EVENT, { detail: result }));
            void syncConfiguredStateChange('feishu_bridge_in').catch(() => undefined);
          }
        })
        .catch(() => {
          // Feishu bridge status is visible in Settings; automatic checks stay quiet.
        })
        .finally(() => {
          feishuSyncInFlight = false;
        });
    }

    async function runAutoSync() {
      if (syncInFlight) {
        return;
      }

      lastAutoSyncAt = Date.now();
      syncInFlight = true;
      await autoSyncConfiguredDatabase()
        .then((result) => {
          if (disposed || (result.skipped_reason && SILENT_AUTO_SYNC_SKIP_REASONS.has(result.skipped_reason))) {
            return;
          }

          setLastAutoSyncMessage(result.message);
        })
        .catch((reason) => {
          if (disposed) {
            return;
          }

          setLastAutoSyncMessage(reason instanceof Error ? reason.message : String(reason));
        })
        .finally(() => {
          syncInFlight = false;
        });

      void runFeishuSync('auto');
    }

    async function getRequiredInterval() {
      try {
        const state = await getStudyModeState();
        const active = state.status === 'active' && ['focus', 'awaiting_break', 'break'].includes(state.phase);
        return active ? ACTIVE_AUTO_SYNC_INTERVAL_MS : AUTO_SYNC_INTERVAL_MS;
      } catch {
        return AUTO_SYNC_INTERVAL_MS;
      }
    }

    const startupTimerId = window.setTimeout(() => {
      void runAutoSync();
    }, AUTO_SYNC_STARTUP_DELAY_MS);

    const intervalId = window.setInterval(() => {
      void (async () => {
        const requiredInterval = await getRequiredInterval();
        if (Date.now() - lastAutoSyncAt >= requiredInterval) {
          await runAutoSync();
        }
        if (Date.now() - lastFeishuAutoSyncAt >= FEISHU_AUTO_SYNC_INTERVAL_MS) {
          await runFeishuSync('auto_poll');
        }
      })();
    }, ACTIVE_AUTO_SYNC_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearTimeout(startupTimerId);
      window.clearInterval(intervalId);
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    void listenTauriEvent<{ active_state_changed?: boolean; took_over_active_mode?: boolean }>(
      STUDY_SYNC_STATE_CHANGED_EVENT,
      (event) => {
        if (event.payload?.took_over_active_mode) {
          setActivePage('focus');
        }
      },
    ).then((dispose) => {
      unlisten = dispose;
    }).catch(() => {
      // Browser previews and partial desktop runtimes should not surface event wiring noise.
    });

    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let disposed = false;
    let updateCheckInFlight = false;

    async function runAutoUpdateCheck() {
      if (updateCheckInFlight) {
        return;
      }

      updateCheckInFlight = true;
      try {
        const update = await checkForAppUpdate();
        if (disposed) {
          return;
        }

        if (update === null) {
          setLastAutoUpdateMessage('自动检查更新：当前已经是最新版本。');
          return;
        }

        const message = `自动检查更新：发现新版本 ${update.version}，可在这里下载并安装。`;
        setLastAutoUpdateMessage(message);

        const lastNotifiedVersion = window.localStorage.getItem(AUTO_UPDATE_NOTICE_STORAGE_KEY);
        if (lastNotifiedVersion !== update.version) {
          window.localStorage.setItem(AUTO_UPDATE_NOTICE_STORAGE_KEY, update.version);
          void showStudyReminder(
            '发现新版本',
            `考研专注 ${update.version} 可更新，打开设置页下载并安装。`,
            'silent',
            `update:${update.version}`,
          ).catch(() => {
            // Settings page also shows the automatic check result; notification is best-effort.
          });
        }
      } catch (reason) {
        if (!disposed) {
          setLastAutoUpdateMessage(`自动检查更新失败：${reason instanceof Error ? reason.message : String(reason)}`);
        }
      } finally {
        updateCheckInFlight = false;
      }
    }

    const startupTimerId = window.setTimeout(() => {
      void runAutoUpdateCheck();
    }, AUTO_UPDATE_CHECK_STARTUP_DELAY_MS);
    const intervalId = window.setInterval(() => {
      void runAutoUpdateCheck();
    }, AUTO_UPDATE_CHECK_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearTimeout(startupTimerId);
      window.clearInterval(intervalId);
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const remindedKeys = new Set<string>();

    async function checkScheduleReminders() {
      try {
        const date = todayString();
        const nowMinute = currentMinuteOfDay();
        const pageData = await getSchedulePageData(date);
        if (disposed) return;

        for (const block of pageData.day_blocks) {
          if (block.status === 'completed') continue;
          const distance = nowMinute - block.start_minute;
          if (distance < 0 || distance > SCHEDULE_REMINDER_LOOKBACK_MINUTES) continue;

          const key = `${block.id}:${block.schedule_date}:${block.start_minute}`;
          if (remindedKeys.has(key)) continue;
          remindedKeys.add(key);

          void notifyStudyReminder({
            title: '课表时间到了',
            body: `${block.title} 已到开始时间，可以回到课表一键开始专注。`,
          });
        }

        for (const key of [...remindedKeys]) {
          if (!key.includes(`:${date}:`)) {
            remindedKeys.delete(key);
          }
        }
      } catch {
        // Schedule reminders are best-effort and should never interrupt the app.
      }
    }

    void checkScheduleReminders();
    const intervalId = window.setInterval(() => {
      void checkScheduleReminders();
    }, SCHEDULE_REMINDER_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const notifiedAlarmIds = new Set<number>();

    async function refreshNextAlarm() {
      try {
        const alarm = await getNextAlarm();
        if (!disposed) {
          setNextAlarm(alarm);
        }
      } catch {
        // Alarm status is visible on the Alarm page; the global widget stays quiet.
      }
    }

    async function checkAlarms() {
      try {
        const ringingAlarms = await triggerDueAlarms();
        if (disposed) return;

        for (const alarm of ringingAlarms) {
          const key = `alarm:${alarm.id}`;
          if (notifiedAlarmIds.has(alarm.id)) {
            continue;
          }
          notifiedAlarmIds.add(alarm.id);
          void notifyPersistentAlarm(key, {
            title: alarm.title,
            body: alarm.note?.trim() || `闹钟时间到了：${alarm.alarm_date} ${alarm.alarm_time}`,
          });
        }

        for (const id of [...notifiedAlarmIds]) {
          if (!ringingAlarms.some((alarm) => alarm.id === id)) {
            notifiedAlarmIds.delete(id);
            stopPersistentAlarmSound(`alarm:${id}`);
          }
        }

        await refreshNextAlarm();
        if (ringingAlarms.length > 0) {
          notifyAlarmStateChanged();
        }
      } catch {
        // Alarm checks are best-effort and should never interrupt the app.
      }
    }

    void refreshNextAlarm();
    void checkAlarms();
    const intervalId = window.setInterval(() => {
      void checkAlarms();
    }, ALARM_CHECK_INTERVAL_MS);

    const handleAlarmStateChanged = () => {
      void refreshNextAlarm();
    };
    window.addEventListener(ALARM_STATE_CHANGED_EVENT, handleAlarmStateChanged);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
      window.removeEventListener(ALARM_STATE_CHANGED_EVENT, handleAlarmStateChanged);
      stopPersistentAlarmSound();
    };
  }, []);

  useEffect(() => {
    let disposed = false;

    async function checkEmailReminders() {
      try {
        const result = await checkDueTaskEmailReminders();
        if (!disposed && result.status === 'sent') {
          setLastAutoSyncMessage(result.message);
        }
      } catch {
        // Email reminder failures are visible from Settings test/save; the periodic check stays quiet.
      }
    }

    void checkEmailReminders();
    const intervalId = window.setInterval(() => {
      void checkEmailReminders();
    }, EMAIL_REMINDER_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
    };
  }, []);

  function handleThemeChange(nextTheme: AppTheme) {
    setTheme(nextTheme);
    void getAppSettings()
      .then((settings) => saveAppSettings({ ...settings, ui_theme: nextTheme }))
      .catch(() => {
        // Local theme storage is still applied immediately; database persistence can be retried from Settings.
      });
  }

  function renderActivePage() {
    if (activePage === 'settings') {
      return (
        <SettingsPage
          lastAutoSyncMessage={lastAutoSyncMessage}
          lastAutoUpdateMessage={lastAutoUpdateMessage}
          theme={theme}
          onThemeChange={handleThemeChange}
        />
      );
    }

    return pages[activePage].component;
  }

  return (
    <Layout
      activePage={activePage}
      nextAlarm={nextAlarm}
      pages={pages}
      onNavigate={setActivePage}
      theme={theme}
      onThemeChange={handleThemeChange}
    >
      {renderActivePage()}
    </Layout>
  );
}

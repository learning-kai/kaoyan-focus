import { type ReactNode, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { BarChart3, CalendarDays, ClipboardList, NotebookPen, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import ChecklistPage from './pages/ChecklistPage';
import SchedulePage from './pages/SchedulePage';
import ReviewPage from './pages/ReviewPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
import { notifyStudyReminder } from './services/alertApi';
import { getStudyModeState } from './services/focusApi';
import { getSchedulePageData } from './services/scheduleApi';
import {
  STUDY_SYNC_STATE_CHANGED_EVENT,
  autoSyncConfiguredDatabase,
  checkDueTaskEmailReminders,
  getAppSettings,
  saveAppSettings,
  syncConfiguredStateChange,
  syncFeishuBridge,
} from './services/settingsApi';
import type { AppPage } from './types/navigation';
import { applyTheme, bootstrapTheme, storeTheme } from './theme';
import type { AppTheme } from './types/settings';

const AUTO_SYNC_STARTUP_DELAY_MS = 5000;
const AUTO_SYNC_INTERVAL_MS = 3 * 60 * 1000;
const ACTIVE_AUTO_SYNC_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_LOOKBACK_MINUTES = 1;
const EMAIL_REMINDER_INTERVAL_MS = 5 * 60 * 1000;
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

      if (!feishuSyncInFlight) {
        feishuSyncInFlight = true;
        void syncFeishuBridge('auto')
          .then((result) => {
            if (result.status === 'synced' && (result.pulled_count > 0 || result.deleted_count > 0)) {
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
    }

    async function getRequiredInterval() {
      try {
        const [state, settings] = await Promise.all([getStudyModeState(), getAppSettings()]);
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

    void listen<{ active_state_changed?: boolean; took_over_active_mode?: boolean }>(
      STUDY_SYNC_STATE_CHANGED_EVENT,
      (event) => {
        if (event.payload?.took_over_active_mode) {
          setActivePage('focus');
        }
      },
    ).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      unlisten?.();
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
      return <SettingsPage lastAutoSyncMessage={lastAutoSyncMessage} theme={theme} onThemeChange={handleThemeChange} />;
    }

    return pages[activePage].component;
  }

  return (
    <Layout activePage={activePage} pages={pages} onNavigate={setActivePage} theme={theme} onThemeChange={handleThemeChange}>
      {renderActivePage()}
    </Layout>
  );
}

import { type ReactNode, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { BarChart3, ClipboardList, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import ChecklistPage from './pages/ChecklistPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
import { getStudyModeState } from './services/focusApi';
import { STUDY_SYNC_STATE_CHANGED_EVENT, autoSyncConfiguredDatabase, getAppSettings, saveAppSettings } from './services/settingsApi';
import type { AppPage } from './types/navigation';
import { applyTheme, bootstrapTheme, storeTheme } from './theme';
import type { AppTheme } from './types/settings';

const AUTO_SYNC_STARTUP_DELAY_MS = 5000;
const AUTO_SYNC_INTERVAL_MS = 3 * 60 * 1000;
const ACTIVE_AUTO_SYNC_INTERVAL_MS = 30 * 1000;
const SILENT_AUTO_SYNC_SKIP_REASONS = new Set([
  'webdav_not_configured',
  'webdav_disabled',
  'object_storage_not_configured',
  'object_storage_disabled',
  'study_mode_active',
]);

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
    description: '阶段计划与今日执行',
    icon: ClipboardList,
    component: <ChecklistPage />,
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
    }

    async function getRequiredInterval() {
      try {
        const [state, settings] = await Promise.all([getStudyModeState(), getAppSettings()]);
        const active = state.status === 'active' && ['focus', 'awaiting_break', 'break'].includes(state.phase);
        return active || settings.sync_backend === 'object_storage' ? ACTIVE_AUTO_SYNC_INTERVAL_MS : AUTO_SYNC_INTERVAL_MS;
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

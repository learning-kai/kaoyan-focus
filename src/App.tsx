import { Suspense, useEffect, useState } from 'react';
import AppErrorBoundary from './components/AppErrorBoundary';
import Layout from './components/Layout';
import { pages } from './navigation';
import { APP_NAVIGATE_EVENT } from './navigationEvents';
import {
  useAlarmWatcher,
  useAutoSync,
  useAutoUpdateCheck,
  useEmailReminders,
  useScheduleReminders,
  useSyncTakeoverNavigation,
} from './hooks/useAppBackgroundTasks';
import { getAppSettings, saveAppSettings } from './services/settingsApi';
import type { AppPage } from './types/navigation';
import { applyTheme, bootstrapTheme, storeTheme } from './theme';
import type { Alarm } from './types/alarm';
import type { AppTheme } from './types/settings';

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
    function handleAppNavigation(event: Event) {
      const page = (event as CustomEvent<{ page?: AppPage }>).detail?.page;
      if (page && page in pages) {
        setActivePage(page);
      }
    }

    window.addEventListener(APP_NAVIGATE_EVENT, handleAppNavigation);
    return () => window.removeEventListener(APP_NAVIGATE_EVENT, handleAppNavigation);
  }, []);

  useAutoSync(setLastAutoSyncMessage);
  useSyncTakeoverNavigation(setActivePage);
  useAutoUpdateCheck(setLastAutoUpdateMessage);
  useScheduleReminders();
  useAlarmWatcher(setNextAlarm);
  useEmailReminders(setLastAutoSyncMessage);

  function handleThemeChange(nextTheme: AppTheme) {
    setTheme(nextTheme);
    void getAppSettings()
      .then((settings) => saveAppSettings({ ...settings, ui_theme: nextTheme }))
      .catch(() => {
        // Local theme storage is still applied immediately; database persistence can be retried from Settings.
      });
  }

  function renderActivePage() {
    const ActivePage = pages[activePage].component;

    return (
      <Suspense
        fallback={
          <section className="page-shell page-loading-shell" aria-live="polite">
            <p className="eyebrow">Loading</p>
            <h2>正在加载页面...</h2>
          </section>
        }
      >
        {activePage === 'settings' ? (
          <ActivePage
            lastAutoSyncMessage={lastAutoSyncMessage}
            lastAutoUpdateMessage={lastAutoUpdateMessage}
            theme={theme}
            onThemeChange={handleThemeChange}
          />
        ) : (
          <ActivePage />
        )}
      </Suspense>
    );
  }

  return (
    <AppErrorBoundary>
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
    </AppErrorBoundary>
  );
}

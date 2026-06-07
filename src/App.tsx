import { useEffect, useState } from 'react';
import Layout from './components/Layout';
import SettingsPage from './pages/SettingsPage';
import { pages } from './navigation';
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

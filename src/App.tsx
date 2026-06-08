import { Suspense, useCallback, useEffect, useRef, useState } from 'react';
import AppErrorBoundary from './components/AppErrorBoundary';
import Layout from './components/Layout';
import { getPageFromKeyboardShortcut, pages } from './navigation';
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

const APP_TITLE = '考研专注';
const ACTIVE_PAGE_STORAGE_KEY = 'kaoyan-focus-active-page';

function isAppPage(value: string | null | undefined): value is AppPage {
  return typeof value === 'string' && Object.prototype.hasOwnProperty.call(pages, value);
}

function isKeyboardNavigationBlocked(target: EventTarget | null) {
  if (document.querySelector('[aria-modal="true"], dialog[open], [role="dialog"], [data-block-global-shortcuts="true"]')) {
    return true;
  }

  if (!(target instanceof Element)) {
    return false;
  }

  return Boolean(
    target.closest(
      'input, textarea, select, [contenteditable], [role="menu"], [role="listbox"], [role="tree"], [role="grid"], [data-block-global-shortcuts="true"]',
    ),
  );
}

function getPageFromHash(): AppPage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    const hashPage = decodeURIComponent(window.location.hash.replace(/^#/, ''));
    return isAppPage(hashPage) ? hashPage : null;
  } catch {
    return null;
  }
}

function getStoredPage(): AppPage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    const storedPage = window.localStorage.getItem(ACTIVE_PAGE_STORAGE_KEY);
    return isAppPage(storedPage) ? storedPage : null;
  } catch {
    return null;
  }
}

function getInitialPage(): AppPage {
  return getPageFromHash() ?? getStoredPage() ?? 'focus';
}

export default function App() {
  const [activePage, setActivePage] = useState<AppPage>(() => getInitialPage());
  const [lastAutoSyncMessage, setLastAutoSyncMessage] = useState<string | null>(null);
  const [lastAutoUpdateMessage, setLastAutoUpdateMessage] = useState<string | null>(null);
  const [nextAlarm, setNextAlarm] = useState<Alarm | null>(null);
  const [theme, setTheme] = useState<AppTheme>(() => bootstrapTheme());
  const hasSyncedPageRef = useRef(false);
  const navigateToPage = useCallback((page: AppPage) => {
    setActivePage(page);
  }, []);

  useEffect(() => {
    applyTheme(theme);
    storeTheme(theme);
  }, [theme]);

  useEffect(() => {
    const pageTitle = pages[activePage].title;
    document.title = activePage === 'focus' ? APP_TITLE : `${pageTitle} · ${APP_TITLE}`;

    try {
      window.localStorage.setItem(ACTIVE_PAGE_STORAGE_KEY, activePage);
    } catch {
      // Navigation remains fully usable when local storage is unavailable.
    }

    const nextHash = `#${activePage}`;
    const nextUrl = `${window.location.pathname}${window.location.search}${nextHash}`;

    if (!hasSyncedPageRef.current) {
      hasSyncedPageRef.current = true;
      if (window.location.hash !== nextHash) {
        window.history.replaceState(null, '', nextUrl);
      }
      return;
    }

    if (window.location.hash !== nextHash) {
      window.location.hash = activePage;
    }
  }, [activePage]);

  useEffect(() => {
    function handleAppNavigation(event: Event) {
      const page = (event as CustomEvent<{ page?: AppPage }>).detail?.page;
      if (isAppPage(page)) {
        navigateToPage(page);
      }
    }

    window.addEventListener(APP_NAVIGATE_EVENT, handleAppNavigation);
    return () => window.removeEventListener(APP_NAVIGATE_EVENT, handleAppNavigation);
  }, [navigateToPage]);

  useEffect(() => {
    function handleHistoryNavigation() {
      const page = getPageFromHash();
      if (page) {
        navigateToPage(page);
      }
    }

    window.addEventListener('hashchange', handleHistoryNavigation);
    window.addEventListener('popstate', handleHistoryNavigation);
    return () => {
      window.removeEventListener('hashchange', handleHistoryNavigation);
      window.removeEventListener('popstate', handleHistoryNavigation);
    };
  }, [navigateToPage]);

  useEffect(() => {
    function handleKeyboardNavigation(event: KeyboardEvent) {
      if (event.defaultPrevented || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }

      if (isKeyboardNavigationBlocked(event.target) || isKeyboardNavigationBlocked(document.activeElement)) {
        return;
      }

      const page = getPageFromKeyboardShortcut(event.key);
      if (!page) {
        return;
      }

      event.preventDefault();
      navigateToPage(page);
    }

    window.addEventListener('keydown', handleKeyboardNavigation);
    return () => window.removeEventListener('keydown', handleKeyboardNavigation);
  }, [navigateToPage]);

  useAutoSync(setLastAutoSyncMessage);
  useSyncTakeoverNavigation(navigateToPage);
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
        onNavigate={navigateToPage}
        theme={theme}
        onThemeChange={handleThemeChange}
      >
        {renderActivePage()}
      </Layout>
    </AppErrorBoundary>
  );
}

import { Suspense, useCallback, useEffect, useRef, useState } from 'react';
import AppErrorBoundary from './components/AppErrorBoundary';
import AiPlanDrawer from './components/AiPlanDrawer';
import Layout from './components/Layout';
import MicroBreakOverlay from './components/focus/MicroBreakOverlay';
import { useMicroBreakListener } from './hooks/useMicroBreak';
import UpdateNotification, { type UpdateInfo } from './components/UpdateNotification';
import { getPageFromKeyboardShortcut, pages } from './navigation';
import { APP_NAVIGATE_EVENT } from './navigationEvents';
import { notifyAiPlanApplied, onAiPlanOpen, type AiPlanCategoryLabels } from './services/aiPlanBus';
import {
  useAutoSync,
  useAutoUpdateCheck,
  useAlarmWatcher,
  useEmailReminders,
  useScheduleReminders,
  useStudyModeReminders,
  useSyncTakeoverNavigation,
} from './hooks/useAppBackgroundTasks';
import { getAppSettings, saveAppSettings } from './services/settingsApi';
import type { AppPage } from './types/navigation';
import { applyTheme, bootstrapTheme, storeTheme } from './theme';
import type { Alarm } from './types/alarm';
import type { AppTheme } from './types/settings';

const APP_TITLE = '考研专注';
const ACTIVE_PAGE_STORAGE_KEY = 'kaoyan-focus-active-page';

/** 本地时区的今天，YYYY-MM-DD；仅作抽屉初始值，实际打开时页面总会传入目标日期。 */
function localDateKey(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  return `${now.getFullYear()}-${month}-${day}`;
}

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
  const [pendingUpdate, setPendingUpdate] = useState<UpdateInfo | null>(null);
  const [, setNextAlarm] = useState<Alarm | null>(null);
  const [alarmFocusId, setAlarmFocusId] = useState<number | null>(null);
  const [theme, setTheme] = useState<AppTheme>(() => bootstrapTheme());
  // AI 排期抽屉是 App 级单例：切页时页面组件卸载，抽屉不能跟着被关掉。
  const [aiPlanOpen, setAiPlanOpen] = useState(false);
  const [aiPlanTargetDate, setAiPlanTargetDate] = useState<string>(() => localDateKey());
  const [aiPlanCategoryLabels, setAiPlanCategoryLabels] = useState<AiPlanCategoryLabels | undefined>(
    undefined,
  );
  const hasSyncedPageRef = useRef(false);
  const navigateToPage = useCallback((page: AppPage, options?: { alarmId?: number }) => {
    setActivePage(page);
    if (page !== 'alarm') {
      setAlarmFocusId(null);
      return;
    }

    if (options?.alarmId != null) {
      setAlarmFocusId(options.alarmId);
    }
  }, []);
  const clearAlarmFocusTarget = useCallback(() => {
    setAlarmFocusId(null);
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

  // AI 排期抽屉：页面按钮只发事件，抽屉本体挂在这里，跨页保活。
  useEffect(() => {
    return onAiPlanOpen((targetDate, categoryLabels) => {
      setAiPlanTargetDate(targetDate);
      if (categoryLabels) {
        setAiPlanCategoryLabels(categoryLabels);
      }
      setAiPlanOpen(true);
    });
  }, []);

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
  useStudyModeReminders();
  useAutoUpdateCheck(setLastAutoUpdateMessage, useCallback((update: UpdateInfo) => {
    setPendingUpdate(update);
  }, []));
  useScheduleReminders();
  useAlarmWatcher(setNextAlarm);
  useEmailReminders(setLastAutoSyncMessage);
  useMicroBreakListener();

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
        ) : activePage === 'alarm' ? (
          <ActivePage focusAlarmId={alarmFocusId} onFocusAlarmHandled={clearAlarmFocusTarget} />
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
        skipMainContentFocus={activePage === 'alarm'}
        pages={pages}
        onNavigate={navigateToPage}
      >
        {renderActivePage()}
      </Layout>
      <MicroBreakOverlay />
      <UpdateNotification
        update={pendingUpdate}
        onDismiss={() => setPendingUpdate(null)}
        onUpdateInstalled={() => setPendingUpdate(null)}
      />
      <AiPlanDrawer
        categoryLabels={aiPlanCategoryLabels}
        isOpen={aiPlanOpen}
        onApplied={notifyAiPlanApplied}
        onClose={() => setAiPlanOpen(false)}
        targetDate={aiPlanTargetDate}
      />
    </AppErrorBoundary>
  );
}

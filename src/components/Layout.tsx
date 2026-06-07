import type { PropsWithChildren } from 'react';
import { AlarmClock, BookOpenCheck, CircleDot, Lock, MoonStar, MonitorUp, SunMedium } from 'lucide-react';
import type { PageMeta } from '../App';
import type { Alarm } from '../types/alarm';
import type { AppPage } from '../types/navigation';
import type { AppTheme } from '../types/settings';
import { DESKTOP_RUNTIME_MESSAGE, isTauriRuntime } from '../services/tauriInvoke';

type LayoutProps = PropsWithChildren<{
  activePage: AppPage;
  nextAlarm: Alarm | null;
  pages: Record<AppPage, PageMeta>;
  onNavigate: (page: AppPage) => void;
  theme: AppTheme;
  onThemeChange: (theme: AppTheme) => void;
}>;

function formatNextAlarm(alarm: Alarm | null) {
  if (!alarm) {
    return '暂无闹钟';
  }

  return `${alarm.alarm_date} ${alarm.alarm_time}`;
}

export default function Layout({ activePage, nextAlarm, pages, onNavigate, theme, onThemeChange, children }: LayoutProps) {
  const activeMeta = pages[activePage];
  const desktopReady = isTauriRuntime();
  const runtimeStatusTitle = desktopReady ? '后台待命' : '桌面壳未连接';
  const runtimeStatusText = desktopReady ? '托盘运行 / 本地数据' : '请在 Windows 桌面壳中运行';

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            <BookOpenCheck size={24} />
          </span>
          <div>
            <h1>考研专注</h1>
            <p>Study Console</p>
          </div>
        </div>

        <nav className="nav-list" aria-label="主导航">
          {(Object.keys(pages) as AppPage[]).map((page) => {
            const Icon = pages[page].icon;

            return (
              <button
                aria-current={page === activePage ? 'page' : undefined}
                className={page === activePage ? 'nav-item active' : 'nav-item'}
                key={page}
                onClick={() => onNavigate(page)}
                type="button"
              >
                <Icon size={19} />
                <span>
                  <strong>{pages[page].title}</strong>
                  <small>{pages[page].description}</small>
                </span>
              </button>
            );
          })}
        </nav>

        <div className="sidebar-foot">
          <span className="status-dot" />
          <div>
            <strong>{runtimeStatusTitle}</strong>
            <span title={desktopReady ? undefined : DESKTOP_RUNTIME_MESSAGE}>{runtimeStatusText}</span>
          </div>
        </div>
      </aside>

      <main className="main-panel">
        <div className="top-strip">
          <div className="top-strip-title">
            <CircleDot size={14} />
            <span>{activeMeta.title}</span>
          </div>
          <div className="top-strip-status">
            <span className={desktopReady ? 'runtime-pill is-ready' : 'runtime-pill is-preview'}>
              <MonitorUp size={14} /> {desktopReady ? 'Windows 桌面壳' : '浏览器预览'}
            </span>
            <span className={nextAlarm ? 'next-alarm-pill active' : 'next-alarm-pill'}>
              <AlarmClock size={14} /> {formatNextAlarm(nextAlarm)}
            </span>
            <span><Lock size={14} /> 学习中自动锁定配置</span>
          </div>
          <div className="theme-toggle" role="group" aria-label="主题切换">
            <button
              aria-pressed={theme === 'dark'}
              className={theme === 'dark' ? 'active' : ''}
              onClick={() => onThemeChange('dark')}
              type="button"
            >
              <MoonStar size={14} />
              黑色
            </button>
            <button
              aria-pressed={theme === 'light'}
              className={theme === 'light' ? 'active' : ''}
              onClick={() => onThemeChange('light')}
              type="button"
            >
              <SunMedium size={14} />
              白色
            </button>
          </div>
        </div>
        {children}
      </main>
    </div>
  );
}

import { useEffect, useRef, type MouseEvent, type PropsWithChildren } from 'react';
import { AlarmClock, BookOpenCheck, CircleDot, Lock, MonitorUp } from 'lucide-react';
import type { PageMeta } from '../navigation';
import type { Alarm } from '../types/alarm';
import type { AppPage } from '../types/navigation';
import type { AppTheme } from '../types/settings';
import { DESKTOP_RUNTIME_MESSAGE, isTauriRuntime } from '../services/tauriInvoke';
import { APP_THEME_OPTIONS } from '../theme';

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
  const mainContentRef = useRef<HTMLElement>(null);
  const activeMeta = pages[activePage];
  const desktopReady = isTauriRuntime();
  const runtimeStatusTitle = desktopReady ? '后台待命' : '桌面模式未连接';
  const runtimeStatusText = desktopReady ? '托盘运行 / 本地数据' : '请在 Windows 桌面应用中运行';
  const primaryPages: AppPage[] = ['focus', 'checklist', 'schedule', 'whitelist', 'review'];
  const secondaryPages: AppPage[] = ['stats', 'alarm', 'settings'];

  useEffect(() => {
    mainContentRef.current?.focus({ preventScroll: true });
  }, [activePage]);

  function handleSkipLinkClick(event: MouseEvent<HTMLAnchorElement>) {
    event.preventDefault();
    mainContentRef.current?.focus({ preventScroll: true });
  }

  function renderNavButton(page: AppPage) {
    const meta = pages[page];
    const Icon = meta.icon;

    return (
      <button
        aria-current={page === activePage ? 'page' : undefined}
        aria-keyshortcuts={meta.shortcut}
        aria-label={`${meta.title}：${meta.description}，快捷键 ${meta.shortcut}`}
        className={page === activePage ? 'nav-item active' : 'nav-item'}
        key={page}
        onClick={() => onNavigate(page)}
        title={`${meta.title} · ${meta.description} · ${meta.shortcut}`}
        type="button"
      >
        <Icon size={19} />
        <span>
          <strong>
            <span className="nav-title-full">{meta.title}</span>
            <span className="nav-title-short">{meta.shortTitle ?? meta.title}</span>
          </strong>
          <small>{meta.description}</small>
        </span>
      </button>
    );
  }

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content" onClick={handleSkipLinkClick}>跳到主内容</a>
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            <BookOpenCheck size={24} />
          </span>
          <div>
            <h1>考研专注</h1>
            <p>本地学习控制台</p>
          </div>
        </div>

        <nav className="nav-list" aria-label="主导航">
          <div className="nav-group">
            <p className="nav-group-label">学习闭环</p>
            {primaryPages.map(renderNavButton)}
          </div>

          <div className="nav-group">
            <p className="nav-group-label">辅助能力</p>
            {secondaryPages.map(renderNavButton)}
          </div>
        </nav>

        <div className="sidebar-foot">
          <span className="status-dot" />
          <div>
            <strong>{runtimeStatusTitle}</strong>
            <span title={desktopReady ? undefined : DESKTOP_RUNTIME_MESSAGE}>{runtimeStatusText}</span>
          </div>
        </div>
      </aside>

      <main className="main-panel" id="main-content" ref={mainContentRef} tabIndex={-1}>
        <div className="top-strip">
          <div className="top-strip-title">
            <CircleDot size={14} />
            <span className="top-strip-title-copy">
              <strong>{activeMeta.title}</strong>
              <small>{activeMeta.description}</small>
            </span>
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
            {APP_THEME_OPTIONS.map((option) => (
              <button
                aria-pressed={theme === option.id}
                className={theme === option.id ? 'active' : ''}
                key={option.id}
                onClick={() => onThemeChange(option.id)}
                title={option.description}
                type="button"
              >
                <span
                  aria-hidden="true"
                  className="theme-swatch"
                  style={{ background: `linear-gradient(135deg, ${option.swatch[0]}, ${option.swatch[1]} 58%, ${option.swatch[2]})` }}
                />
                {option.shortLabel}
              </button>
            ))}
          </div>
        </div>
        <div className="page-transition" key={activePage}>
          {children}
        </div>
      </main>
    </div>
  );
}

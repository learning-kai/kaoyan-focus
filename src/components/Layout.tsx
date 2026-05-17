import type { PropsWithChildren } from 'react';
import { BookOpenCheck, CircleDot, Lock, MoonStar, MonitorUp, SunMedium } from 'lucide-react';
import type { PageMeta } from '../App';
import type { AppPage } from '../types/navigation';
import type { AppTheme } from '../types/settings';

type LayoutProps = PropsWithChildren<{
  activePage: AppPage;
  pages: Record<AppPage, PageMeta>;
  onNavigate: (page: AppPage) => void;
  theme: AppTheme;
  onThemeChange: (theme: AppTheme) => void;
}>;

export default function Layout({ activePage, pages, onNavigate, theme, onThemeChange, children }: LayoutProps) {
  const activeMeta = pages[activePage];

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
            <strong>后台待命</strong>
            <span>托盘运行 / 本地数据</span>
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
            <span><MonitorUp size={14} /> Windows 桌面</span>
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

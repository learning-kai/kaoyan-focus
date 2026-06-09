import { useEffect, useRef, type MouseEvent, type PropsWithChildren } from 'react';
import { BookOpenCheck } from 'lucide-react';
import type { PageMeta } from '../navigation';
import type { AppPage } from '../types/navigation';

type LayoutProps = PropsWithChildren<{
  activePage: AppPage;
  skipMainContentFocus?: boolean;
  pages: Record<AppPage, PageMeta>;
  onNavigate: (page: AppPage, options?: { alarmId?: number }) => void;
}>;

export default function Layout({
  activePage,
  pages,
  onNavigate,
  skipMainContentFocus = false,
  children,
}: LayoutProps) {
  const mainContentRef = useRef<HTMLElement>(null);
  const primaryPages: AppPage[] = ['focus', 'checklist', 'schedule', 'whitelist', 'review'];
  const secondaryPages: AppPage[] = ['stats', 'alarm', 'settings'];

  useEffect(() => {
    if (skipMainContentFocus) {
      return;
    }

    mainContentRef.current?.focus({ preventScroll: true });
  }, [activePage, skipMainContentFocus]);

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
      </aside>

      <main className="main-panel" id="main-content" ref={mainContentRef} tabIndex={-1}>
        <div className="page-transition" key={activePage}>
          {children}
        </div>
      </main>
    </div>
  );
}

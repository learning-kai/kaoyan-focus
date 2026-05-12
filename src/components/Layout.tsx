import type { PropsWithChildren } from 'react';
import type { AppPage } from '../types/navigation';

type LayoutProps = PropsWithChildren<{
  activePage: AppPage;
  pages: Record<AppPage, { title: string }>;
  onNavigate: (page: AppPage) => void;
}>;

export default function Layout({ activePage, pages, onNavigate, children }: LayoutProps) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">研</span>
          <div>
            <h1>考研专注</h1>
            <p>Windows 自律学习工具</p>
          </div>
        </div>

        <nav className="nav-list" aria-label="主导航">
          {(Object.keys(pages) as AppPage[]).map((page) => (
            <button
              className={page === activePage ? 'nav-item active' : 'nav-item'}
              key={page}
              onClick={() => onNavigate(page)}
              type="button"
            >
              {pages[page].title}
            </button>
          ))}
        </nav>
      </aside>

      <main className="main-panel">{children}</main>
    </div>
  );
}

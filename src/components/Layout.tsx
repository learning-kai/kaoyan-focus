import type { PropsWithChildren } from 'react';
import { BookOpenCheck } from 'lucide-react';
import type { PageMeta } from '../App';
import type { AppPage } from '../types/navigation';

type LayoutProps = PropsWithChildren<{
  activePage: AppPage;
  pages: Record<AppPage, PageMeta>;
  onNavigate: (page: AppPage) => void;
}>;

export default function Layout({ activePage, pages, onNavigate, children }: LayoutProps) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            <BookOpenCheck size={24} />
          </span>
          <div>
            <h1>考研专注</h1>
            <p>Windows 学习约束控制台</p>
          </div>
        </div>

        <nav className="nav-list" aria-label="主导航">
          {(Object.keys(pages) as AppPage[]).map((page) => {
            const Icon = pages[page].icon;

            return (
              <button
                className={page === activePage ? 'nav-item active' : 'nav-item'}
                key={page}
                onClick={() => onNavigate(page)}
                type="button"
              >
                <Icon size={18} />
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
          <span>本地数据与后台托盘运行</span>
        </div>
      </aside>

      <main className="main-panel">{children}</main>
    </div>
  );
}

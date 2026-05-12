import { type ReactNode, useState } from 'react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
import type { AppPage } from './types/navigation';

const pages: Record<AppPage, { title: string; component: ReactNode }> = {
  focus: { title: '专注', component: <FocusPage /> },
  whitelist: { title: '白名单', component: <WhitelistPage /> },
  stats: { title: '统计', component: <StatsPage /> },
  settings: { title: '设置', component: <SettingsPage /> },
};

export default function App() {
  const [activePage, setActivePage] = useState<AppPage>('focus');

  return (
    <Layout activePage={activePage} pages={pages} onNavigate={setActivePage}>
      {pages[activePage].component}
    </Layout>
  );
}

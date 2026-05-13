import { type ReactNode, useEffect, useState } from 'react';
import { BarChart3, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
import { autoSyncWebDavDatabase } from './services/settingsApi';
import type { AppPage } from './types/navigation';

export type PageMeta = {
  title: string;
  description: string;
  icon: LucideIcon;
  component: ReactNode;
};

const pages: Record<AppPage, PageMeta> = {
  focus: {
    title: '专注',
    description: '学习模式与番茄钟',
    icon: TimerReset,
    component: <FocusPage />,
  },
  whitelist: {
    title: '白名单',
    description: '软件与网站放行',
    icon: ShieldCheck,
    component: <WhitelistPage />,
  },
  stats: {
    title: '统计',
    description: '学习记录与干扰',
    icon: BarChart3,
    component: <StatsPage />,
  },
  settings: {
    title: '设置',
    description: '节奏、同步与更新',
    icon: Settings,
    component: <SettingsPage />,
  },
};

export default function App() {
  const [activePage, setActivePage] = useState<AppPage>('focus');
  const [lastAutoSyncMessage, setLastAutoSyncMessage] = useState<string | null>(null);

  useEffect(() => {
    const timerId = window.setTimeout(() => {
      void autoSyncWebDavDatabase()
        .then((result) => {
          if (result.skipped_reason === 'webdav_not_configured') {
            return;
          }

          setLastAutoSyncMessage(result.message);
        })
        .catch((reason) => {
          setLastAutoSyncMessage(reason instanceof Error ? reason.message : String(reason));
        });
    }, 5000);

    return () => window.clearTimeout(timerId);
  }, []);

  return (
    <Layout activePage={activePage} pages={pages} onNavigate={setActivePage}>
      {activePage === 'settings'
        ? <SettingsPage lastAutoSyncMessage={lastAutoSyncMessage} />
        : pages[activePage].component}
    </Layout>
  );
}

import { type ReactNode, useState } from 'react';
import { BarChart3, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import Layout from './components/Layout';
import FocusPage from './pages/FocusPage';
import WhitelistPage from './pages/WhitelistPage';
import StatsPage from './pages/StatsPage';
import SettingsPage from './pages/SettingsPage';
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
    description: '学习与干扰记录',
    icon: BarChart3,
    component: <StatsPage />,
  },
  settings: {
    title: '设置',
    description: '默认参数与更新',
    icon: Settings,
    component: <SettingsPage />,
  },
};

export default function App() {
  const [activePage, setActivePage] = useState<AppPage>('focus');

  return (
    <Layout activePage={activePage} pages={pages} onNavigate={setActivePage}>
      {pages[activePage].component}
    </Layout>
  );
}

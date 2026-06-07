import type { ReactNode } from 'react';
import { AlarmClock, BarChart3, CalendarDays, ClipboardList, NotebookPen, Settings, ShieldCheck, TimerReset, type LucideIcon } from 'lucide-react';
import AlarmPage from './pages/AlarmPage';
import ChecklistPage from './pages/ChecklistPage';
import FocusPage from './pages/FocusPage';
import ReviewPage from './pages/ReviewPage';
import SchedulePage from './pages/SchedulePage';
import SettingsPage from './pages/SettingsPage';
import StatsPage from './pages/StatsPage';
import WhitelistPage from './pages/WhitelistPage';
import type { AppPage } from './types/navigation';

export type PageMeta = {
  title: string;
  description: string;
  icon: LucideIcon;
  component: ReactNode;
};

export const pages: Record<AppPage, PageMeta> = {
  focus: {
    title: '专注',
    description: '学习模式与番茄钟',
    icon: TimerReset,
    component: <FocusPage />,
  },
  alarm: {
    title: '闹钟',
    description: '全局一次性提醒',
    icon: AlarmClock,
    component: <AlarmPage />,
  },
  checklist: {
    title: '清单',
    description: '五类待办与今日任务',
    icon: ClipboardList,
    component: <ChecklistPage />,
  },
  schedule: {
    title: '课表',
    description: '今日安排与本周视图',
    icon: CalendarDays,
    component: <SchedulePage />,
  },
  review: {
    title: '复盘',
    description: '每日总结与明日重点',
    icon: NotebookPen,
    component: <ReviewPage />,
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

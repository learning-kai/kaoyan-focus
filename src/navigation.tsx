import { lazy, type ComponentType, type LazyExoticComponent } from 'react';
import {
  AlarmClock,
  BarChart3,
  CalendarDays,
  ClipboardList,
  NotebookPen,
  Settings,
  ShieldCheck,
  TimerReset,
  type LucideIcon,
} from 'lucide-react';
import type { AppPage } from './types/navigation';

export type PageMeta = {
  title: string;
  shortTitle?: string;
  description: string;
  shortcut: string;
  icon: LucideIcon;
  component: LazyExoticComponent<ComponentType<any>>;
};

export const pages: Record<AppPage, PageMeta> = {
  focus: {
    title: '专注',
    description: '开始学习与约束',
    shortcut: 'Alt+1',
    icon: TimerReset,
    component: lazy(() => import('./pages/FocusPage')),
  },
  alarm: {
    title: '闹钟',
    description: '全局一次性提醒',
    shortcut: 'Alt+7',
    icon: AlarmClock,
    component: lazy(() => import('./pages/AlarmPage')),
  },
  checklist: {
    title: '清单',
    description: '今天真正要做的事',
    shortcut: 'Alt+2',
    icon: ClipboardList,
    component: lazy(() => import('./pages/ChecklistPage')),
  },
  schedule: {
    title: '课表',
    description: '把任务落到时间',
    shortcut: 'Alt+3',
    icon: CalendarDays,
    component: lazy(() => import('./pages/SchedulePage')),
  },
  review: {
    title: '复盘',
    description: '每日总结与明日重点',
    shortcut: 'Alt+5',
    icon: NotebookPen,
    component: lazy(() => import('./pages/ReviewPage')),
  },
  whitelist: {
    title: '白名单',
    shortTitle: '放行',
    description: '学习前确认放行',
    shortcut: 'Alt+4',
    icon: ShieldCheck,
    component: lazy(() => import('./pages/WhitelistPage')),
  },
  stats: {
    title: '统计',
    description: '学习记录与干扰',
    shortcut: 'Alt+6',
    icon: BarChart3,
    component: lazy(() => import('./pages/StatsPage')),
  },
  settings: {
    title: '设置',
    description: '节奏、同步与更新',
    shortcut: 'Alt+8',
    icon: Settings,
    component: lazy(() => import('./pages/SettingsPage')),
  },
};

const pageShortcutMap = Object.entries(pages).reduce<Record<string, AppPage>>((shortcuts, [page, meta]) => {
  const shortcutKey = getAltShortcutKey(meta.shortcut);
  if (shortcutKey) {
    shortcuts[shortcutKey] = page as AppPage;
  }
  return shortcuts;
}, Object.create(null) as Record<string, AppPage>);

function getAltShortcutKey(shortcut: string) {
  const match = /^Alt\+(.+)$/i.exec(shortcut.trim());
  return match?.[1]?.toLowerCase() ?? null;
}

export function getPageFromKeyboardShortcut(key: string): AppPage | null {
  const shortcutKey = key.toLowerCase();
  return Object.prototype.hasOwnProperty.call(pageShortcutMap, shortcutKey) ? pageShortcutMap[shortcutKey] : null;
}

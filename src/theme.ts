import type { AppTheme } from './types/settings';

const THEME_STORAGE_KEY = 'kaoyan-focus-theme';

export type AppThemeOption = {
  id: AppTheme;
  label: string;
  shortLabel: string;
  description: string;
  swatch: [string, string, string];
};

export const APP_THEME_OPTIONS: AppThemeOption[] = [
  {
    id: 'dark',
    label: '黑色',
    shortLabel: '黑色',
    description: '深色控制台，高对比专注感。',
    swatch: ['#08111d', '#79a7ff', '#4fd0a1'],
  },
  {
    id: 'light',
    label: '白色磨砂',
    shortLabel: '白色',
    description: '清爽明亮，带轻量玻璃质感。',
    swatch: ['#f8fbff', '#3869d4', '#239a75'],
  },
  {
    id: 'mono',
    label: '水墨纸面',
    shortLabel: '水墨',
    description: '低饱和纸面质感，适合长时间阅读。',
    swatch: ['#f2f1ed', '#26282d', '#8a928b'],
  },
  {
    id: 'dawn',
    label: '晨光琥珀',
    shortLabel: '晨光',
    description: '明亮、温和，保留清晰任务边界。',
    swatch: ['#f7fbf8', '#356fd6', '#ef9160'],
  },
  {
    id: 'forest',
    label: '清新浅绿',
    shortLabel: '森林',
    description: '清新淡绿，低饱和明亮，适合长时间学习。',
    swatch: ['#f4fbf6', '#75b68f', '#9edcc1'],
  },
  {
    id: 'sakura',
    label: '樱色晨雾',
    shortLabel: '樱色',
    description: '柔和亮色，粉蓝点缀但不过分甜腻。',
    swatch: ['#f8fbff', '#5579df', '#d47791'],
  },
];

export function normalizeTheme(value: string | null | undefined): AppTheme {
  return APP_THEME_OPTIONS.some((option) => option.id === value) ? (value as AppTheme) : 'light';
}

function getThemeOption(theme: AppTheme) {
  return APP_THEME_OPTIONS.find((option) => option.id === theme) ?? APP_THEME_OPTIONS[1];
}

export function getThemeColor(theme: AppTheme) {
  return getThemeOption(theme).swatch[0];
}

export function readStoredTheme(): AppTheme {
  if (typeof window === 'undefined') {
    return 'light';
  }

  try {
    return normalizeTheme(window.localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return 'light';
  }
}

export function storeTheme(theme: AppTheme) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Best effort only.
  }
}

export function applyTheme(theme: AppTheme) {
  if (typeof document === 'undefined') {
    return;
  }

  document.documentElement.dataset.theme = theme;
  document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')?.setAttribute('content', getThemeColor(theme));
}

export function bootstrapTheme() {
  const theme = readStoredTheme();
  applyTheme(theme);
  return theme;
}

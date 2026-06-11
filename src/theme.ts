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
    id: 'light',
    label: '浅色',
    shortLabel: '浅色',
    description: '干净明亮，Apple 风格。',
    swatch: ['#ffffff', '#007aff', '#34c759'],
  },
  {
    id: 'dark',
    label: '深色',
    shortLabel: '深色',
    description: '纯黑背景，高对比专注感。',
    swatch: ['#000000', '#0a84ff', '#30d158'],
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

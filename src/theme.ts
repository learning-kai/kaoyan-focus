import type { AppTheme } from './types/settings';

const THEME_STORAGE_KEY = 'kaoyan-focus-theme';

export function normalizeTheme(value: string | null | undefined): AppTheme {
  return value === 'dark' ? 'dark' : 'light';
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
}

export function bootstrapTheme() {
  const theme = readStoredTheme();
  applyTheme(theme);
  return theme;
}

import type { AppPage } from './types/navigation';

export const APP_NAVIGATE_EVENT = 'kaoyan-focus:navigate';

export function requestAppNavigation(page: AppPage) {
  window.dispatchEvent(new CustomEvent<{ page: AppPage }>(APP_NAVIGATE_EVENT, {
    detail: { page },
  }));
}

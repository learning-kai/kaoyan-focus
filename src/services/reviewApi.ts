import type { DailyReview, DailyReviewDraft, DailyReviewPageData } from '../types/review';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function getDailyReviewPageData(reviewDate?: string | null): Promise<DailyReviewPageData> {
  return invokeCommand<DailyReviewPageData>('get_daily_review_page_data', { reviewDate });
}

export function saveDailyReview(draft: DailyReviewDraft): Promise<DailyReview> {
  return invokeCommand<DailyReview>('save_daily_review', { draft });
}

export function deleteDailyReview(reviewId: number): Promise<void> {
  return invokeCommand<void>('delete_daily_review', { reviewId });
}

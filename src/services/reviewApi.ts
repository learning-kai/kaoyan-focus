import type {
  DailyReview,
  DailyReviewDraft,
  DailyReviewPageData,
  WeeklyReview,
  WeeklyReviewDraft,
  WeeklyReviewPageData,
} from '../types/review';
import { invokeCommand } from './tauriInvoke';

export function getDailyReviewPageData(reviewDate?: string | null): Promise<DailyReviewPageData> {
  return invokeCommand<DailyReviewPageData>('get_daily_review_page_data', { reviewDate });
}

export function saveDailyReview(draft: DailyReviewDraft): Promise<DailyReview> {
  return invokeCommand<DailyReview>('save_daily_review', { draft });
}

export function deleteDailyReview(reviewId: number): Promise<void> {
  return invokeCommand<void>('delete_daily_review', { reviewId });
}

export function getWeeklyReviewPageData(weekDate?: string | null): Promise<WeeklyReviewPageData> {
  return invokeCommand<WeeklyReviewPageData>('get_weekly_review_page_data', { weekDate });
}

export function saveWeeklyReview(draft: WeeklyReviewDraft): Promise<WeeklyReview> {
  return invokeCommand<WeeklyReview>('save_weekly_review', { draft });
}

export function deleteWeeklyReview(reviewId: number): Promise<void> {
  return invokeCommand<void>('delete_weekly_review', { reviewId });
}

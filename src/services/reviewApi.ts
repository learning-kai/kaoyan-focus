import { parseAiSchedulerError } from './aiSchedulerApi';
import type {
  AiReviewDraft,
  DailyReview,
  DailyReviewDraft,
  DailyReviewPageData,
  ReviewAiScope,
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

/**
 * 让 AI 起草一份复盘。
 *
 * 后端返回的错误是 `AiSchedulerError` JSON 信封，这里统一解码成结构化错误对象，
 * 与 AI 排期页面的处理方式保持一致。
 */
export function draftReviewWithAi(scope: ReviewAiScope, reviewDate?: string | null): Promise<AiReviewDraft> {
  return invokeCommand<AiReviewDraft>('draft_review_with_ai', {
    scope,
    reviewDate: reviewDate ?? null,
  }).catch((raw: unknown) => {
    throw parseAiSchedulerError(raw);
  });
}

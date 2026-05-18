export type DailyReview = {
  id: number;
  review_date: string;
  summary: string | null;
  blockers: string | null;
  tomorrow_focus: string | null;
  mood_score: number;
  created_at: string;
  updated_at: string;
};

export type DailyReviewSummary = {
  study_seconds: number;
  focus_session_count: number;
  interruption_count: number;
  schedule_total: number;
  schedule_completed: number;
  today_total: number;
  today_completed: number;
};

export type DailyReviewPageData = {
  review_date: string;
  review: DailyReview | null;
  summary: DailyReviewSummary;
};

export type DailyReviewDraft = {
  reviewDate: string;
  summary?: string | null;
  blockers?: string | null;
  tomorrowFocus?: string | null;
  moodScore: number;
};

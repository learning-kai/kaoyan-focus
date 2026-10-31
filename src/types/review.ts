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

export type WeeklyReview = {
  id: number;
  week_start_date: string;
  summary: string | null;
  blockers: string | null;
  next_week_focus: string | null;
  mood_score: number;
  created_at: string;
  updated_at: string;
};

export type WeeklyReviewSummary = {
  study_seconds: number;
  focus_session_count: number;
  interruption_count: number;
};

export type WeeklyReviewPageData = {
  week_start_date: string;
  week_end_date: string;
  review: WeeklyReview | null;
  summary: WeeklyReviewSummary;
};

export type WeeklyReviewDraft = {
  weekStartDate: string;
  summary?: string | null;
  blockers?: string | null;
  nextWeekFocus?: string | null;
  moodScore: number;
};

/** AI 复盘起草的结果。字段与 Daily/WeeklyReviewDraft 的文本字段一一对应。 */
export type AiReviewDraft = {
  /** 'daily' | 'weekly' */
  scope: string;
  summary: string;
  blockers: string;
  focus: string;
  /** 本次用到的数据摘要，用于向用户回显发了什么。 */
  context_notes: string[];
  /** 出这次结果的模型名。 */
  model: string;
};

export type ReviewAiScope = 'daily' | 'weekly';

export type ScheduleBlock = {
  id: number;
  schedule_date: string;
  title: string;
  note: string | null;
  category_key: string;
  subject_id: number | null;
  source_today_item_id: number | null;
  template_id: number | null;
  start_minute: number;
  end_minute: number;
  status: 'planned' | 'running' | 'completed' | string;
  linked_study_mode_id: number | null;
  linked_focus_session_id: number | null;
  has_conflict: boolean;
  created_at: string;
  updated_at: string;
};

export type ScheduleTemplate = {
  id: number;
  title: string;
  note: string | null;
  category_key: string;
  subject_id: number | null;
  weekdays: number[];
  start_minute: number;
  end_minute: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type ScheduleTodayItem = {
  id: number;
  title: string;
  note: string | null;
  due_date: string | null;
  subject_id: number | null;
  completed: boolean;
};

export type ScheduleDay = {
  date: string;
  weekday: number;
  blocks: ScheduleBlock[];
  planned_minutes: number;
};

export type SchedulePageData = {
  selected_date: string;
  today_date: string;
  week_start_date: string;
  day_blocks: ScheduleBlock[];
  week_days: ScheduleDay[];
  today_items: ScheduleTodayItem[];
  templates: ScheduleTemplate[];
};

export type ScheduleBlockDraft = {
  scheduleDate: string;
  title: string;
  note?: string | null;
  categoryKey: string;
  subjectId?: number | null;
  sourceTodayItemId?: number | null;
  startMinute: number;
  endMinute: number;
};

export type ScheduleTemplateDraft = {
  title: string;
  note?: string | null;
  categoryKey: string;
  subjectId?: number | null;
  weekdays: number[];
  startMinute: number;
  endMinute: number;
  enabled: boolean;
};

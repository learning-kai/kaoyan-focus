export type ChecklistTask = {
  id: number;
  category_key: string;
  subject_id: number | null;
  title: string;
  note: string | null;
  due_date: string | null;
  sort_order: number;
  completed: boolean;
  created_at: string;
  updated_at: string;
};

export type TodayPlanItem = {
  id: number;
  today_date: string;
  source_task_id: number | null;
  subject_id: number | null;
  title: string;
  note: string | null;
  due_date: string | null;
  sort_order: number;
  completed: boolean;
  synced_source_completion: boolean;
  created_at: string;
  updated_at: string;
};

export type ChecklistCategory = {
  key: string;
  title: string;
  pending_tasks: ChecklistTask[];
  completed_tasks: ChecklistTask[];
  highlighted: boolean;
};

export type ChecklistPageData = {
  today_date: string;
  active_category_key: string;
  highlighted_subject_id: number | null;
  categories: ChecklistCategory[];
  today_items: TodayPlanItem[];
};

export type ChecklistTaskDraft = {
  categoryKey: string;
  title: string;
  note?: string | null;
  dueDate?: string | null;
};

export type TodayPlanItemDraft = {
  title: string;
  note?: string | null;
  dueDate?: string | null;
  subjectId: number | null;
};

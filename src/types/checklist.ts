/** 任务优先级。AI 排期据此决定先后与是否抢占高效时段。 */
export type ChecklistPriority = 'high' | 'medium' | 'low';

export const CHECKLIST_PRIORITY_OPTIONS: Array<{
  value: ChecklistPriority;
  label: string;
}> = [
  { value: 'high', label: '高' },
  { value: 'medium', label: '中' },
  { value: 'low', label: '低' },
];

export type ChecklistTask = {
  id: number;
  category_key: string;
  subject_id: number | null;
  title: string;
  note: string | null;
  due_date: string | null;
  sort_order: number;
  completed: boolean;
  /** 'high' | 'medium' | 'low'，后端已规范化，不会出现其它值。 */
  priority: string;
  /** 预计耗时（分钟）。0 表示未估时，排期时回落到默认时长。 */
  estimated_minutes: number;
  /** AI 排期锚点：为 true 时一键重排不会移动该任务已落定的时间块。 */
  ai_pinned: boolean;
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
  priority: string;
  estimated_minutes: number;
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
  /** 省略即保持后端默认：priority = 'medium'、estimatedMinutes = 0（未估）。 */
  priority?: string | null;
  estimatedMinutes?: number | null;
  /** 仅更新时使用：省略（null/undefined）表示不覆盖已有值。 */
  aiPinned?: boolean | null;
};

export type TodayPlanItemDraft = {
  title: string;
  note?: string | null;
  dueDate?: string | null;
  subjectId: number | null;
  priority?: string | null;
  estimatedMinutes?: number | null;
};

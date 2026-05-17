import type {
  ChecklistPageData,
  ChecklistTask,
  ChecklistTaskDraft,
  TodayPlanItem,
  TodayPlanItemDraft,
} from '../types/checklist';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function getChecklistPageData(): Promise<ChecklistPageData> {
  return invokeCommand<ChecklistPageData>('get_checklist_page_data');
}

export function createChecklistTask(draft: ChecklistTaskDraft): Promise<ChecklistTask> {
  return invokeCommand<ChecklistTask>('create_checklist_task', { draft });
}

export function updateChecklistTask(id: number, draft: ChecklistTaskDraft): Promise<ChecklistTask> {
  return invokeCommand<ChecklistTask>('update_checklist_task', { id, draft });
}

export function deleteChecklistTask(id: number): Promise<void> {
  return invokeCommand<void>('delete_checklist_task', { id });
}

export function reorderChecklistTasks(categoryKey: string, orderedIds: number[]): Promise<void> {
  return invokeCommand<void>('reorder_checklist_tasks', {
    categoryKey,
    orderedIds,
  });
}

export function completeChecklistTask(id: number, completed: boolean): Promise<ChecklistTask> {
  return invokeCommand<ChecklistTask>('complete_checklist_task', { id, completed });
}

export function addTaskToTodayPlan(taskId: number): Promise<TodayPlanItem> {
  return invokeCommand<TodayPlanItem>('add_task_to_today_plan', { taskId });
}

export function createTodayPlanItem(draft: TodayPlanItemDraft): Promise<TodayPlanItem> {
  return invokeCommand<TodayPlanItem>('create_today_plan_item', { draft });
}

export function updateTodayPlanItem(id: number, draft: TodayPlanItemDraft): Promise<TodayPlanItem> {
  return invokeCommand<TodayPlanItem>('update_today_plan_item', { id, draft });
}

export function deleteTodayPlanItem(id: number): Promise<void> {
  return invokeCommand<void>('delete_today_plan_item', { id });
}

export function reorderTodayPlanItems(orderedIds: number[]): Promise<void> {
  return invokeCommand<void>('reorder_today_plan_items', { orderedIds });
}

export function completeTodayPlanItem(id: number, completed: boolean, syncSourceCompletion: boolean): Promise<TodayPlanItem> {
  return invokeCommand<TodayPlanItem>('complete_today_plan_item', {
    id,
    completed,
    syncSourceCompletion,
  });
}

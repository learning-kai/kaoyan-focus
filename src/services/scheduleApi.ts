import type { FocusMode, StudyModeState } from '../types/focus';
import type {
  ScheduleBlock,
  ScheduleBlockDraft,
  SchedulePageData,
  ScheduleTemplate,
  ScheduleTemplateDraft,
} from '../types/schedule';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function getSchedulePageData(selectedDate?: string | null): Promise<SchedulePageData> {
  return invokeCommand<SchedulePageData>('get_schedule_page_data', { selectedDate });
}

export function createScheduleBlock(draft: ScheduleBlockDraft): Promise<ScheduleBlock> {
  return invokeCommand<ScheduleBlock>('create_schedule_block', { draft });
}

export function createScheduleBlockFromTodayItem(
  todayItemId: number,
  scheduleDate: string,
  startMinute: number,
  endMinute: number,
): Promise<ScheduleBlock> {
  return invokeCommand<ScheduleBlock>('create_schedule_block_from_today_item', {
    todayItemId,
    scheduleDate,
    startMinute,
    endMinute,
  });
}

export function updateScheduleBlock(id: number, draft: ScheduleBlockDraft): Promise<ScheduleBlock> {
  return invokeCommand<ScheduleBlock>('update_schedule_block', { id, draft });
}

export function moveScheduleBlock(
  id: number,
  scheduleDate: string,
  startMinute: number,
  endMinute: number,
): Promise<ScheduleBlock> {
  return invokeCommand<ScheduleBlock>('move_schedule_block', {
    id,
    scheduleDate,
    startMinute,
    endMinute,
  });
}

export function deleteScheduleBlock(id: number): Promise<void> {
  return invokeCommand<void>('delete_schedule_block', { id });
}

export function createScheduleTemplate(draft: ScheduleTemplateDraft): Promise<ScheduleTemplate> {
  return invokeCommand<ScheduleTemplate>('create_schedule_template', { draft });
}

export function updateScheduleTemplate(id: number, draft: ScheduleTemplateDraft): Promise<ScheduleTemplate> {
  return invokeCommand<ScheduleTemplate>('update_schedule_template', { id, draft });
}

export function deleteScheduleTemplate(id: number): Promise<void> {
  return invokeCommand<void>('delete_schedule_template', { id });
}

export function startStudyModeFromScheduleBlock(
  blockId: number,
  plannedSeconds: number,
  focusSeconds: number,
  breakSeconds: number,
  longBreakSeconds: number,
  longBreakInterval: number,
  mode: FocusMode,
): Promise<StudyModeState> {
  return invokeCommand<StudyModeState>('start_study_mode_from_schedule_block', {
    blockId,
    plannedSeconds,
    focusSeconds,
    breakSeconds,
    longBreakSeconds,
    longBreakInterval,
    mode,
  });
}

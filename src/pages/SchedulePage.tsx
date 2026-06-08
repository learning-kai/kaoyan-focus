import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  DragEvent as ReactDragEvent,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from 'react';
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Clock3,
  CopyPlus,
  PencilLine,
  Play,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { getAppSettings } from '../services/settingsApi';
import { syncConfiguredStateChange } from '../services/syncApi';
import { FEISHU_SYNC_REFRESH_EVENT, syncFeishuBridge } from '../services/feishuApi';
import {
  createScheduleBlock,
  createScheduleBlockFromTodayItem,
  createScheduleTemplate,
  deleteScheduleBlock,
  deleteScheduleTemplate,
  getSchedulePageData,
  moveScheduleBlock,
  startStudyModeFromScheduleBlock,
  updateScheduleBlock,
  updateScheduleTemplate,
} from '../services/scheduleApi';
import { listSubjects } from '../services/focusApi';
import type { AppSettings } from '../types/settings';
import type { Subject } from '../types/focus';
import type { ScheduleBlock, ScheduleBlockDraft, SchedulePageData, ScheduleTemplate, ScheduleTemplateDraft } from '../types/schedule';
import { currentMinuteOfDay, formatDateKey } from '../utils/date';
import { requestAppNavigation } from '../navigationEvents';

const categories = [
  { key: 'politics', label: '政治' },
  { key: 'english', label: '英语' },
  { key: 'math', label: '数学' },
  { key: 'major', label: '专业课' },
  { key: 'general', label: '通用' },
];

const weekdays = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const dayStart = 6 * 60;
const dayEnd = 24 * 60;
const slotMinutes = 15;
const minBlockMinutes = 15;
const defaultBlockMinutes = 60;
const todayItemDragType = 'application/x-schedule-today-item';
const quickScheduleSlots = [
  { label: '上午', minute: 8 * 60 },
  { label: '下午', minute: 14 * 60 },
  { label: '晚上', minute: 19 * 60 },
];

const emptyBlockDraft = (date: string): ScheduleBlockDraft => ({
  scheduleDate: date,
  title: '',
  note: '',
  categoryKey: 'general',
  subjectId: null,
  sourceTodayItemId: null,
  startMinute: 8 * 60,
  endMinute: 9 * 60,
});

const emptyTemplateDraft: ScheduleTemplateDraft = {
  title: '',
  note: '',
  categoryKey: 'general',
  subjectId: null,
  weekdays: [1, 2, 3, 4, 5],
  startMinute: 8 * 60,
  endMinute: 9 * 60,
  enabled: true,
};

function shiftDate(value: string, days: number) {
  const [year, month, day] = value.split('-').map(Number);
  if (!year || !month || !day) {
    return value;
  }

  const date = new Date(year, month - 1, day);
  date.setDate(date.getDate() + days);
  const nextYear = date.getFullYear();
  const nextMonth = String(date.getMonth() + 1).padStart(2, '0');
  const nextDay = String(date.getDate()).padStart(2, '0');
  return `${nextYear}-${nextMonth}-${nextDay}`;
}

function formatMinute(minute: number) {
  const safe = Math.max(0, Math.min(24 * 60, minute));
  return `${String(Math.floor(safe / 60)).padStart(2, '0')}:${String(safe % 60).padStart(2, '0')}`;
}

function parseTime(value: string) {
  const [hour, minute] = value.split(':').map(Number);
  return (Number.isFinite(hour) ? hour : 0) * 60 + (Number.isFinite(minute) ? minute : 0);
}

function timelinePercent(minute: number) {
  return Math.max(0, Math.min(100, ((minute - dayStart) / (dayEnd - dayStart)) * 100));
}

function rangeTimelineStyle(startMinute: number, endMinute: number) {
  const visibleStart = Math.max(dayStart, Math.min(dayEnd, startMinute));
  const visibleEnd = Math.max(visibleStart + minBlockMinutes, Math.min(dayEnd, endMinute));
  const height = Math.max(5, ((visibleEnd - visibleStart) / (dayEnd - dayStart)) * 100);
  return {
    top: `${timelinePercent(visibleStart)}%`,
    height: `${height}%`,
  };
}

function blockTimelineStyle(block: ScheduleBlock) {
  return rangeTimelineStyle(block.start_minute, block.end_minute);
}

function clampMinute(value: number, min = dayStart, max = dayEnd) {
  return Math.max(min, Math.min(max, value));
}

function snapMinute(value: number) {
  return Math.round(value / slotMinutes) * slotMinutes;
}

type PositionedScheduleBlock = {
  block: ScheduleBlock;
  columnIndex: number;
  columnCount: number;
};

type CalendarDragState = {
  mode: 'create' | 'move' | 'resize-start' | 'resize-end';
  title: string;
  blockId?: number;
  todayItemId?: number;
  originalStart: number;
  originalEnd: number;
  startMinute: number;
  endMinute: number;
  originClientY?: number;
};

function layoutScheduleBlocks(blocks: ScheduleBlock[]): PositionedScheduleBlock[] {
  const ordered = [...blocks].sort((left, right) =>
    left.start_minute - right.start_minute ||
    left.end_minute - right.end_minute ||
    left.id - right.id,
  );
  const groups: ScheduleBlock[][] = [];
  let activeGroup: ScheduleBlock[] = [];
  let activeGroupEnd = Number.NEGATIVE_INFINITY;

  for (const block of ordered) {
    if (!activeGroup.length || block.start_minute < activeGroupEnd) {
      activeGroup.push(block);
      activeGroupEnd = Math.max(activeGroupEnd, block.end_minute);
    } else {
      groups.push(activeGroup);
      activeGroup = [block];
      activeGroupEnd = block.end_minute;
    }
  }
  if (activeGroup.length) groups.push(activeGroup);

  return groups.flatMap((group) => {
    const columnEnds: number[] = [];
    const assigned = group.map((block) => {
      const reusableColumn = columnEnds.findIndex((endMinute) => endMinute <= block.start_minute);
      const columnIndex = reusableColumn >= 0 ? reusableColumn : columnEnds.length;
      columnEnds[columnIndex] = block.end_minute;
      return { block, columnIndex };
    });
    const columnCount = Math.max(1, columnEnds.length);
    return assigned.map(({ block, columnIndex }) => ({ block, columnIndex, columnCount }));
  });
}

function positionedBlockTimelineStyle(positioned: PositionedScheduleBlock) {
  const base = blockTimelineStyle(positioned.block);
  const gap = 8;
  const sidePadding = 20;
  const totalGap = (positioned.columnCount - 1) * gap;
  const offsetPercent = (positioned.columnIndex * 100) / positioned.columnCount;
  const offsetPixels = positioned.columnIndex * gap - (positioned.columnIndex * (sidePadding + totalGap)) / positioned.columnCount;
  const width = `calc((100% - ${sidePadding}px - ${totalGap}px) / ${positioned.columnCount})`;
  return {
    ...base,
    left: `calc(10px + ${offsetPercent}% + ${offsetPixels}px)`,
    right: 'auto',
    width,
  };
}

function categoryLabel(key: string) {
  return categories.find((item) => item.key === key)?.label ?? '通用';
}

function categoryKeyForSubject(subjectId: number | null) {
  if (subjectId === 1) return 'politics';
  if (subjectId === 2) return 'english';
  if (subjectId === 3) return 'math';
  if (subjectId === 4) return 'major';
  return 'general';
}

function subjectName(subjects: Subject[], subjectId: number | null) {
  return subjectId ? subjects.find((subject) => subject.id === subjectId)?.name ?? '未知科目' : '未指定';
}

function draftFromTemplate(template: ScheduleTemplate): ScheduleTemplateDraft {
  return {
    title: template.title,
    note: template.note,
    categoryKey: template.category_key,
    subjectId: template.subject_id,
    weekdays: template.weekdays,
    startMinute: template.start_minute,
    endMinute: template.end_minute,
    enabled: template.enabled,
  };
}

export default function SchedulePage() {
  const [data, setData] = useState<SchedulePageData | null>(null);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [selectedDate, setSelectedDate] = useState(formatDateKey());
  const [dateDraft, setDateDraft] = useState(formatDateKey());
  const [view, setView] = useState<'day' | 'week'>('day');
  const [blockDraft, setBlockDraft] = useState<ScheduleBlockDraft>(() => emptyBlockDraft(formatDateKey()));
  const [templateDraft, setTemplateDraft] = useState<ScheduleTemplateDraft>(emptyTemplateDraft);
  const [showBlockComposer, setShowBlockComposer] = useState(false);
  const [showTemplateComposer, setShowTemplateComposer] = useState(false);
  const [editingTemplateId, setEditingTemplateId] = useState<number | null>(null);
  const [editingBlockId, setEditingBlockId] = useState<number | null>(null);
  const [editingBlockDraft, setEditingBlockDraft] = useState<ScheduleBlockDraft | null>(null);
  const [quickAddDraft, setQuickAddDraft] = useState<ScheduleBlockDraft | null>(null);
  const [quickAddSourceTodayItemId, setQuickAddSourceTodayItemId] = useState<number | null>(null);
  const [pendingTodayItemId, setPendingTodayItemId] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [loadingSchedule, setLoadingSchedule] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragState, setDragState] = useState<CalendarDragState | null>(null);
  const refreshTokenRef = useRef(0);
  const laneRef = useRef<HTMLDivElement | null>(null);
  const dragStateRef = useRef<CalendarDragState | null>(null);
  const dragFrameRef = useRef<number | null>(null);
  const pendingDragClientYRef = useRef<number | null>(null);
  const dragEffectKey = dragState && dragState.mode !== 'create' ? `${dragState.mode}:${dragState.blockId ?? ''}` : null;

  useEffect(() => {
    void initialize();
  }, []);

  useEffect(() => {
    void refresh(selectedDate);
    setDateDraft(selectedDate);
    setBlockDraft((draft) => ({ ...draft, scheduleDate: selectedDate }));
  }, [selectedDate]);

  useEffect(() => {
    const handleFeishuRefresh = () => {
      void refresh(selectedDate);
    };
    window.addEventListener(FEISHU_SYNC_REFRESH_EVENT, handleFeishuRefresh);
    return () => window.removeEventListener(FEISHU_SYNC_REFRESH_EVENT, handleFeishuRefresh);
  }, [selectedDate]);

  function setCalendarDragState(next: CalendarDragState | null) {
    dragStateRef.current = next;
    setDragState(next);
  }

  useEffect(() => {
    const active = dragStateRef.current;
    if (!active || active.mode === 'create') return;

    function handlePointerMove(event: PointerEvent) {
      event.preventDefault();
      scheduleDragPreview(event.clientY);
    }

    function handlePointerUp() {
      flushScheduledDragPreview();
      void commitDrag(dragStateRef.current);
    }

    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp, { once: true });
    window.addEventListener('pointercancel', handlePointerUp, { once: true });
    return () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerUp);
      window.removeEventListener('pointercancel', handlePointerUp);
      cancelScheduledDragPreview();
    };
  }, [dragEffectKey]);

  const positionedDayBlocks = useMemo(
    () => layoutScheduleBlocks(data?.day_blocks ?? []),
    [data?.day_blocks],
  );

  const currentMinute = useMemo(() => {
    if (selectedDate !== formatDateKey()) return null;
    return currentMinuteOfDay();
  }, [selectedDate]);

  async function initialize() {
    try {
      const [subjectData, appSettings] = await Promise.all([listSubjects(), getAppSettings()]);
      setSubjects(subjectData);
      setSettings(appSettings);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refresh(date = selectedDate) {
    const token = refreshTokenRef.current + 1;
    refreshTokenRef.current = token;
    try {
      setLoadingSchedule(true);
      const pageData = await getSchedulePageData(date);
      if (refreshTokenRef.current === token) {
        setData(pageData);
      }
    } catch (reason) {
      if (refreshTokenRef.current === token) {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    } finally {
      if (refreshTokenRef.current === token) {
        setLoadingSchedule(false);
      }
    }
  }

  function commitDate(value: string) {
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
      setSelectedDate(value);
    }
  }

  async function withSave(action: () => Promise<void>, done: string, trigger = 'local_data_change') {
    try {
      setSaving(true);
      setError(null);
      await action();
      await refresh();
      setMessage(done);
      void syncConfiguredStateChange(trigger).catch(() => undefined);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function handleSyncSchedule() {
    try {
      setSaving(true);
      setError(null);
      setMessage(null);
      await refresh(selectedDate);
      const feishuResult = await syncFeishuBridge('schedule_change');
      await syncConfiguredStateChange('schedule_change').catch(() => undefined);
      await refresh(selectedDate);
      if (feishuResult.status === 'failed') {
        setError(feishuResult.message || '飞书日历同步失败。');
        return;
      }
      setMessage(feishuResult.status === 'synced' ? '课表已同步到飞书日历。' : '课表已刷新，本地修改已自动保存。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  function scheduleSlotEnd(startMinute: number) {
    return Math.min(dayEnd, startMinute + defaultBlockMinutes);
  }

  function minuteFromLaneClientY(clientY: number) {
    const lane = laneRef.current;
    if (!lane) return dayStart;
    const rect = lane.getBoundingClientRect();
    const ratio = rect.height > 0 ? (clientY - rect.top) / rect.height : 0;
    return clampMinute(snapMinute(dayStart + ratio * (dayEnd - dayStart)));
  }

  function minuteDeltaFromClientY(clientY: number, originClientY: number) {
    const lane = laneRef.current;
    if (!lane) return 0;
    const rect = lane.getBoundingClientRect();
    if (rect.height <= 0) return 0;
    return snapMinute(((clientY - originClientY) / rect.height) * (dayEnd - dayStart));
  }

  function nextDragForMinute(current: CalendarDragState, minute: number) {
    if (current.mode === 'create') {
      const startMinute = clampMinute(minute, dayStart, dayEnd - minBlockMinutes);
      return {
        ...current,
        startMinute,
        endMinute: clampMinute(startMinute + defaultBlockMinutes, startMinute + minBlockMinutes, dayEnd),
      };
    }

    if (current.mode === 'move') {
      const duration = current.originalEnd - current.originalStart;
      const startMinute = clampMinute(minute, dayStart, dayEnd - duration);
      return {
        ...current,
        startMinute,
        endMinute: startMinute + duration,
      };
    }

    if (current.mode === 'resize-start') {
      const startMinute = clampMinute(minute, dayStart, current.originalEnd - minBlockMinutes);
      return {
        ...current,
        startMinute,
        endMinute: current.originalEnd,
      };
    }

    const endMinute = clampMinute(minute, current.originalStart + minBlockMinutes, dayEnd);
    return {
      ...current,
      startMinute: current.originalStart,
      endMinute,
    };
  }

  function updateDragPreview(clientY: number) {
    const current = dragStateRef.current;
    if (!current) return;
    let next = current;
    if (current.mode !== 'create' && typeof current.originClientY === 'number') {
      const delta = minuteDeltaFromClientY(clientY, current.originClientY);
      if (current.mode === 'move') {
        const duration = current.originalEnd - current.originalStart;
        const startMinute = clampMinute(current.originalStart + delta, dayStart, dayEnd - duration);
        next = {
          ...current,
          startMinute,
          endMinute: startMinute + duration,
        };
      } else if (current.mode === 'resize-start') {
        const startMinute = clampMinute(current.originalStart + delta, dayStart, current.originalEnd - minBlockMinutes);
        next = {
          ...current,
          startMinute,
          endMinute: current.originalEnd,
        };
      } else {
        const endMinute = clampMinute(current.originalEnd + delta, current.originalStart + minBlockMinutes, dayEnd);
        next = {
          ...current,
          startMinute: current.originalStart,
          endMinute,
        };
      }
    } else {
      next = nextDragForMinute(current, minuteFromLaneClientY(clientY));
    }
    setCalendarDragState(next);
  }

  function cancelScheduledDragPreview() {
    if (dragFrameRef.current !== null) {
      window.cancelAnimationFrame(dragFrameRef.current);
      dragFrameRef.current = null;
    }
    pendingDragClientYRef.current = null;
  }

  function flushScheduledDragPreview() {
    const pendingClientY = pendingDragClientYRef.current;
    cancelScheduledDragPreview();
    if (typeof pendingClientY === 'number') {
      updateDragPreview(pendingClientY);
    }
  }

  function scheduleDragPreview(clientY: number) {
    pendingDragClientYRef.current = clientY;
    if (dragFrameRef.current !== null) {
      return;
    }

    dragFrameRef.current = window.requestAnimationFrame(() => {
      const pendingClientY = pendingDragClientYRef.current;
      dragFrameRef.current = null;
      pendingDragClientYRef.current = null;
      if (typeof pendingClientY === 'number') {
        updateDragPreview(pendingClientY);
      }
    });
  }

  function startCreateDrag(itemId: number, title: string, clientY?: number) {
    setView('day');
    setQuickAddDraft(null);
    setQuickAddSourceTodayItemId(null);
    setPendingTodayItemId(null);
    setMessage(null);
    const startMinute = typeof clientY === 'number' ? minuteFromLaneClientY(clientY) : dayStart;
    setCalendarDragState({
      mode: 'create',
      title,
      todayItemId: itemId,
      originalStart: startMinute,
      originalEnd: scheduleSlotEnd(startMinute),
      startMinute,
      endMinute: scheduleSlotEnd(startMinute),
    });
  }

  function startBlockDrag(block: ScheduleBlock, mode: CalendarDragState['mode'], clientY: number) {
    if (mode === 'create') return;
    setEditingBlockId(null);
    setEditingBlockDraft(null);
    setQuickAddDraft(null);
    setQuickAddSourceTodayItemId(null);
    setMessage(null);
    setCalendarDragState({
      mode,
      title: block.title,
      blockId: block.id,
      originalStart: block.start_minute,
      originalEnd: block.end_minute,
      startMinute: block.start_minute,
      endMinute: block.end_minute,
      originClientY: clientY,
    });
  }

  async function commitDrag(state: CalendarDragState | null = dragState) {
    if (!state) return;
    setCalendarDragState(null);
    if (state.mode !== 'create' && state.startMinute === state.originalStart && state.endMinute === state.originalEnd) return;
    await withSave(async () => {
      if (state.mode === 'create' && typeof state.todayItemId === 'number') {
        await createScheduleBlockFromTodayItem(state.todayItemId, selectedDate, state.startMinute, state.endMinute);
      } else if (typeof state.blockId === 'number') {
        await moveScheduleBlock(state.blockId, selectedDate, state.startMinute, state.endMinute);
      }
    }, state.mode === 'create' ? '今日任务已安排到课表。' : '课表时间已更新。');
  }

  function isInteractiveElement(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    return Boolean(target.closest('button, input, select, textarea, a, .schedule-resize-handle'));
  }

  function handleTodayItemDragStart(event: ReactDragEvent<HTMLElement>, itemId: number, title: string) {
    event.dataTransfer.effectAllowed = 'copy';
    event.dataTransfer.setData(todayItemDragType, String(itemId));
    event.dataTransfer.setData('text/plain', title);
    startCreateDrag(itemId, title, event.clientY);
  }

  function handleTodayItemDragEnd() {
    const active = dragStateRef.current;
    if (active?.mode === 'create') {
      cancelScheduledDragPreview();
      setCalendarDragState(null);
    }
  }

  function handleLaneDragOver(event: ReactDragEvent<HTMLDivElement>) {
    if (!event.dataTransfer.types.includes(todayItemDragType)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
    const current = dragStateRef.current;
    const itemId = Number(event.dataTransfer.getData(todayItemDragType) || current?.todayItemId);
    const item = data?.today_items.find((candidate) => candidate.id === itemId);
    if (!item) return;
    if (!current || current.mode !== 'create' || current.todayItemId !== item.id) {
      startCreateDrag(item.id, item.title, event.clientY);
      return;
    }
    scheduleDragPreview(event.clientY);
  }

  function handleLaneDrop(event: ReactDragEvent<HTMLDivElement>) {
    if (!event.dataTransfer.types.includes(todayItemDragType)) return;
    event.preventDefault();
    flushScheduledDragPreview();
    const active = dragStateRef.current;
    if (active?.mode === 'create') {
      void commitDrag(active);
    }
  }

  function handleBlockPointerDown(event: ReactPointerEvent<HTMLElement>, block: ScheduleBlock) {
    if (event.button !== 0 || editingBlockId === block.id || isInteractiveElement(event.target)) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    event.preventDefault();
    startBlockDrag(block, 'move', event.clientY);
  }

  function handleResizePointerDown(
    event: ReactPointerEvent<HTMLButtonElement>,
    block: ScheduleBlock,
    mode: 'resize-start' | 'resize-end',
  ) {
    if (event.button !== 0 || editingBlockId === block.id) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    event.preventDefault();
    event.stopPropagation();
    startBlockDrag(block, mode, event.clientY);
  }

  function nextKeyboardBlockTime(block: ScheduleBlock, key: string, shiftKey: boolean) {
    const duration = block.end_minute - block.start_minute;

    if (!shiftKey) {
      if (key !== 'ArrowUp' && key !== 'ArrowDown' && key !== 'ArrowLeft' && key !== 'ArrowRight') return null;
      const delta = key === 'ArrowUp' || key === 'ArrowLeft' ? -slotMinutes : slotMinutes;
      const startMinute = clampMinute(block.start_minute + delta, dayStart, dayEnd - duration);
      return {
        startMinute,
        endMinute: startMinute + duration,
      };
    }

    if (key === 'ArrowUp') {
      return {
        startMinute: clampMinute(block.start_minute - slotMinutes, dayStart, block.end_minute - minBlockMinutes),
        endMinute: block.end_minute,
      };
    }

    if (key === 'ArrowDown') {
      return {
        startMinute: clampMinute(block.start_minute + slotMinutes, dayStart, block.end_minute - minBlockMinutes),
        endMinute: block.end_minute,
      };
    }

    if (key === 'ArrowLeft') {
      return {
        startMinute: block.start_minute,
        endMinute: clampMinute(block.end_minute - slotMinutes, block.start_minute + minBlockMinutes, dayEnd),
      };
    }

    if (key === 'ArrowRight') {
      return {
        startMinute: block.start_minute,
        endMinute: clampMinute(block.end_minute + slotMinutes, block.start_minute + minBlockMinutes, dayEnd),
      };
    }

    return null;
  }

  function handleBlockKeyDown(event: ReactKeyboardEvent<HTMLElement>, block: ScheduleBlock) {
    if (saving || editingBlockId === block.id || isInteractiveElement(event.target)) return;

    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      beginEditBlock(block);
      return;
    }

    if (event.key === 'Delete') {
      event.preventDefault();
      event.stopPropagation();
      void withSave(() => deleteScheduleBlock(block.id), '课表块已删除。');
      return;
    }

    const nextTime = nextKeyboardBlockTime(block, event.key, event.shiftKey);
    if (!nextTime) return;

    event.preventDefault();
    event.stopPropagation();
    if (nextTime.startMinute === block.start_minute && nextTime.endMinute === block.end_minute) return;
    void withSave(
      async () => {
        await moveScheduleBlock(block.id, selectedDate, nextTime.startMinute, nextTime.endMinute);
      },
      '课表时间已更新。',
    );
  }

  function quickDraftForSlot(startMinute: number, itemId: number | null = null): ScheduleBlockDraft {
    const item = itemId ? data?.today_items.find((candidate) => candidate.id === itemId) : null;
    const subjectId = item?.subject_id ?? null;
    return {
      ...emptyBlockDraft(selectedDate),
      title: item?.title ?? '',
      note: item?.note ?? '',
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
      sourceTodayItemId: itemId,
      startMinute,
      endMinute: scheduleSlotEnd(startMinute),
    };
  }

  function openQuickAddAt(startMinute: number, itemId: number | null = null) {
    setView('day');
    setShowBlockComposer(false);
    setQuickAddSourceTodayItemId(itemId);
    setQuickAddDraft(quickDraftForSlot(startMinute, itemId));
    setMessage(null);
  }

  async function handleTimeSlotClick(startMinute: number) {
    if (pendingTodayItemId !== null) {
      await handleAddTodayItemAt(pendingTodayItemId, startMinute);
      return;
    }

    openQuickAddAt(startMinute);
  }

  function resetTemplateEditor() {
    setEditingTemplateId(null);
    setTemplateDraft(emptyTemplateDraft);
    setMessage(null);
  }

  function applySubjectToDraft(subjectId: number | null) {
    setBlockDraft((draft) => ({
      ...draft,
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
    }));
  }

  function applyTemplateSubject(subjectId: number | null) {
    setTemplateDraft((draft) => ({
      ...draft,
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
    }));
  }

  function applyQuickSubject(subjectId: number | null) {
    setQuickAddDraft((draft) => draft ? ({
      ...draft,
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
    }) : draft);
  }

  function handleQuickSourceChange(value: string) {
    if (!quickAddDraft) return;
    const itemId = value ? Number(value) : null;
    setQuickAddSourceTodayItemId(itemId);
    if (itemId === null) {
      setQuickAddDraft({
        ...quickAddDraft,
        title: '',
        note: '',
        sourceTodayItemId: null,
      });
      return;
    }

    const item = data?.today_items.find((candidate) => candidate.id === itemId);
    if (!item) return;
    setQuickAddDraft({
      ...quickAddDraft,
      title: item.title,
      note: item.note ?? '',
      sourceTodayItemId: item.id,
      subjectId: item.subject_id,
      categoryKey: categoryKeyForSubject(item.subject_id),
    });
  }

  function validateBlockDraft(draft: ScheduleBlockDraft, label: string) {
    if (!draft.title.trim()) {
      setMessage(null);
      setError(`${label}需要先填写标题。`);
      return false;
    }
    if (draft.endMinute <= draft.startMinute) {
      setMessage(null);
      setError(`${label}的结束时间必须晚于开始时间。`);
      return false;
    }
    return true;
  }

  function validateTemplateDraft(draft: ScheduleTemplateDraft) {
    if (!draft.title.trim()) {
      setMessage(null);
      setError('周模板需要先填写标题。');
      return false;
    }
    if (draft.endMinute <= draft.startMinute) {
      setMessage(null);
      setError('周模板的结束时间必须晚于开始时间。');
      return false;
    }
    if (draft.weekdays.length === 0) {
      setMessage(null);
      setError('周模板至少需要选择一个生效日期。');
      return false;
    }
    return true;
  }

  async function handleCreateBlock() {
    if (!validateBlockDraft(blockDraft, '课表块')) return;
    await withSave(async () => {
      await createScheduleBlock(blockDraft);
      setBlockDraft(emptyBlockDraft(selectedDate));
      setShowBlockComposer(false);
    }, '课表块已添加。');
  }

  async function handleAddTodayItem(itemId: number) {
    setView('day');
    setQuickAddDraft(null);
    setQuickAddSourceTodayItemId(null);
    setPendingTodayItemId((current) => (current === itemId ? null : itemId));
    setMessage('点击时间轴上的 15 分钟空格，或直接拖动任务到时间轴上。');
  }

  async function handleAddTodayItemAt(itemId: number, startMinute: number) {
    await withSave(async () => {
      await createScheduleBlockFromTodayItem(itemId, selectedDate, startMinute, scheduleSlotEnd(startMinute));
      setPendingTodayItemId(null);
    }, '今日任务已安排到课表。');
  }

  async function handleQuickAddSave() {
    if (!quickAddDraft) return;
    if (quickAddSourceTodayItemId === null && !validateBlockDraft(quickAddDraft, '快速安排')) return;
    if (quickAddSourceTodayItemId !== null && quickAddDraft.endMinute <= quickAddDraft.startMinute) {
      setMessage(null);
      setError('快速安排的结束时间必须晚于开始时间。');
      return;
    }
    await withSave(async () => {
      if (quickAddSourceTodayItemId !== null) {
        await createScheduleBlockFromTodayItem(
          quickAddSourceTodayItemId,
          selectedDate,
          quickAddDraft.startMinute,
          quickAddDraft.endMinute,
        );
      } else {
        await createScheduleBlock(quickAddDraft);
      }
      setQuickAddDraft(null);
      setQuickAddSourceTodayItemId(null);
      setPendingTodayItemId(null);
    }, quickAddSourceTodayItemId !== null ? '今日任务已安排到课表。' : '课表块已添加。');
  }

  function cancelTemplateEdit() {
    resetTemplateEditor();
    setShowTemplateComposer(false);
  }

  function handleToggleTemplateComposer() {
    if (showTemplateComposer) {
      cancelTemplateEdit();
      return;
    }
    resetTemplateEditor();
    setShowTemplateComposer(true);
  }

  function beginEditTemplate(template: ScheduleTemplate) {
    setEditingTemplateId(template.id);
    setTemplateDraft(draftFromTemplate(template));
    setShowTemplateComposer(true);
    setMessage(null);
  }

  async function handleSaveTemplate() {
    if (!validateTemplateDraft(templateDraft)) return;
    await withSave(async () => {
      if (editingTemplateId !== null) {
        await updateScheduleTemplate(editingTemplateId, templateDraft);
      } else {
        await createScheduleTemplate(templateDraft);
      }
      resetTemplateEditor();
      setShowTemplateComposer(false);
    }, editingTemplateId !== null ? '周模板已更新。' : '周模板已保存。');
  }

  async function handleDeleteTemplate(templateId: number) {
    await withSave(async () => {
      await deleteScheduleTemplate(templateId);
      if (editingTemplateId === templateId) {
        cancelTemplateEdit();
      }
    }, '模板已删除。');
  }

  function beginEditBlock(block: ScheduleBlock) {
    setEditingBlockId(block.id);
    setEditingBlockDraft({
      scheduleDate: block.schedule_date,
      title: block.title,
      note: block.note ?? '',
      categoryKey: block.category_key,
      subjectId: block.subject_id,
      sourceTodayItemId: block.source_today_item_id,
      startMinute: block.start_minute,
      endMinute: block.end_minute,
    });
  }

  async function handleUpdateBlock() {
    if (!editingBlockId || !editingBlockDraft) return;
    if (!validateBlockDraft(editingBlockDraft, '课表块')) return;
    await withSave(async () => {
      await updateScheduleBlock(editingBlockId, editingBlockDraft);
      setEditingBlockId(null);
      setEditingBlockDraft(null);
    }, '课表块已更新。');
  }

  async function handleStart(block: ScheduleBlock) {
    const appSettings = settings ?? await getAppSettings();
    await withSave(async () => {
      await startStudyModeFromScheduleBlock(
        block.id,
        appSettings.default_study_minutes * 60,
        appSettings.default_focus_minutes * 60,
        appSettings.break_minutes * 60,
        appSettings.long_break_minutes * 60,
        appSettings.long_break_interval,
        appSettings.default_focus_mode,
      );
      requestAppNavigation('focus');
    }, '已从课表开始专注。', 'focus_state_change');
  }

  return (
    loadingSchedule && data === null ? (
      <section className="page-shell schedule-page">
        <div className="empty-state">
          <strong>正在载入课表</strong>
          <p>正在读取今日安排、周模板和同步状态。</p>
        </div>
      </section>
    ) : (
    <div className="schedule-page">
      <section className="schedule-hero">
        <div>
          <p className="eyebrow">日程安排</p>
          <h2>今日课表</h2>
          <p>把今日任务和手动安排放进一天的时间轴。新增、拖拽、删除会自动保存；需要时可手动同步飞书/云端。</p>
        </div>
        <div className="schedule-actions">
          <button className="primary-button" disabled={saving || loadingSchedule} type="button" onClick={() => void handleSyncSchedule()}>
            <RefreshCw size={16} /> 同步课表
          </button>
          <button className="ghost-button" type="button" onClick={() => void refresh()}>
            <RefreshCw size={16} /> 刷新
          </button>
          <button className="primary-button" type="button" onClick={() => setShowBlockComposer((value) => !value)}>
            <Plus size={16} /> 时间块
          </button>
        </div>
      </section>

      {(error || message) && (
        <div
          aria-live={error ? undefined : 'polite'}
          className={error ? 'alert error' : 'alert success'}
          role={error ? 'alert' : 'status'}
        >
          {error ?? message}
        </div>
      )}

      <section className="schedule-toolbar soft-panel">
        <div className="segmented-control">
          <button className={view === 'day' ? 'active' : ''} type="button" onClick={() => setView('day')}>今日课表</button>
          <button className={view === 'week' ? 'active' : ''} type="button" onClick={() => setView('week')}>本周视图</button>
        </div>
        <div className="date-stepper">
          <button type="button" aria-label="前一天" onClick={() => setSelectedDate(shiftDate(selectedDate, -1))}>
            <ChevronLeft size={16} />
          </button>
          <input
            type="date"
            value={dateDraft}
            onBlur={(event) => commitDate(event.target.value)}
            onChange={(event) => {
              setDateDraft(event.target.value);
              commitDate(event.target.value);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') commitDate(event.currentTarget.value);
            }}
          />
          <button type="button" aria-label="后一天" onClick={() => setSelectedDate(shiftDate(selectedDate, 1))}>
            <ChevronRight size={16} />
          </button>
        </div>
        <button className="ghost-button" type="button" onClick={handleToggleTemplateComposer}>
          <CopyPlus size={16} /> 周模板
        </button>
      </section>

      {loadingSchedule && <div className="schedule-loading-hint">正在更新课表...</div>}

      {showBlockComposer && (
        <section className="schedule-composer soft-panel">
          <input
            placeholder="安排标题"
            value={blockDraft.title}
            onChange={(event) => setBlockDraft({ ...blockDraft, title: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleCreateBlock();
            }}
          />
          <select value={blockDraft.subjectId ?? ''} onChange={(event) => applySubjectToDraft(event.target.value ? Number(event.target.value) : null)}>
            <option value="">未指定科目</option>
            {subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.name}</option>)}
          </select>
          <input type="time" value={formatMinute(blockDraft.startMinute)} onChange={(event) => setBlockDraft({ ...blockDraft, startMinute: parseTime(event.target.value) })} />
          <input type="time" value={formatMinute(blockDraft.endMinute)} onChange={(event) => setBlockDraft({ ...blockDraft, endMinute: parseTime(event.target.value) })} />
          <input placeholder="备注" value={blockDraft.note ?? ''} onChange={(event) => setBlockDraft({ ...blockDraft, note: event.target.value })} />
          <button className="primary-button" disabled={saving} type="button" onClick={() => void handleCreateBlock()}>保存</button>
        </section>
      )}

      {showTemplateComposer && (
        <section className="schedule-composer template-composer soft-panel">
          <input
            placeholder="模板标题"
            value={templateDraft.title}
            onChange={(event) => setTemplateDraft({ ...templateDraft, title: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleSaveTemplate();
            }}
          />
          <select value={templateDraft.subjectId ?? ''} onChange={(event) => applyTemplateSubject(event.target.value ? Number(event.target.value) : null)}>
            <option value="">未指定科目</option>
            {subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.name}</option>)}
          </select>
          <input type="time" value={formatMinute(templateDraft.startMinute)} onChange={(event) => setTemplateDraft({ ...templateDraft, startMinute: parseTime(event.target.value) })} />
          <input type="time" value={formatMinute(templateDraft.endMinute)} onChange={(event) => setTemplateDraft({ ...templateDraft, endMinute: parseTime(event.target.value) })} />
          <div className="weekday-pills">
            {weekdays.map((label, index) => {
              const day = index + 1;
              const active = templateDraft.weekdays.includes(day);
              return (
                <button
                  className={active ? 'active' : ''}
                  key={label}
                  type="button"
                  onClick={() => setTemplateDraft((draft) => ({
                    ...draft,
                    weekdays: active ? draft.weekdays.filter((item) => item !== day) : [...draft.weekdays, day],
                  }))}
                >
                  {label}
                </button>
              );
            })}
          </div>
          <button className="primary-button" disabled={saving} type="button" onClick={() => void handleSaveTemplate()}>{editingTemplateId !== null ? '更新模板' : '保存模板'}</button>
          {editingTemplateId !== null && <button className="ghost-button" disabled={saving} type="button" onClick={cancelTemplateEdit}>取消编辑</button>}
        </section>
      )}

      <div className={`schedule-grid-shell is-${view}`}>
        <aside className="today-task-rail soft-panel">
          <div className="panel-title compact-title">
            <div>
              <p className="eyebrow">今日待排</p>
              <h3>今日任务</h3>
            </div>
          </div>
          {data?.today_items.length ? data.today_items.map((item) => {
            const picking = pendingTodayItemId === item.id;
            return (
              <article
                className={`schedule-task-row${picking ? ' picking' : ''}`}
                draggable={!saving}
                key={item.id}
                onDragEnd={handleTodayItemDragEnd}
                onDragStart={(event) => handleTodayItemDragStart(event, item.id, item.title)}
              >
                <div>
                  <strong>{item.title}</strong>
                  <span>{subjectName(subjects, item.subject_id)}{item.due_date ? ` / ${item.due_date}` : ''}</span>
                </div>
                <div className="schedule-task-actions">
                  <button disabled={saving} type="button" onClick={() => void handleAddTodayItem(item.id)}>
                    {picking ? '取消' : '选时间'}
                  </button>
                  <div className="schedule-quick-slots" aria-label={`${item.title} 快捷安排`}>
                    {quickScheduleSlots.map((slot) => (
                      <button
                        aria-label={`安排 ${item.title} 到${slot.label} ${formatMinute(slot.minute)}`}
                        disabled={saving}
                        key={slot.label}
                        type="button"
                        onClick={() => void handleAddTodayItemAt(item.id, slot.minute)}
                      >
                        {slot.label}
                      </button>
                    ))}
                  </div>
                </div>
              </article>
            );
          }) : <div className="empty-state compact">今日任务为空。</div>}
          {pendingTodayItemId !== null && <div className="schedule-placement-hint">点击右侧时间轴空格，或直接用任务卡上的上午/下午/晚上快捷安排。</div>}
        </aside>

        {view === 'day' ? (
          <section className="schedule-timeline soft-panel">
            <div className="schedule-time-column">
              {Array.from({ length: (dayEnd - dayStart) / 60 + 1 }, (_, index) => {
                const minute = dayStart + index * 60;
                return (
                  <span key={minute} style={{ top: `${timelinePercent(minute)}%` }}>
                    {formatMinute(minute)}
                  </span>
                );
              })}
            </div>
            <div
              className={`schedule-lane${pendingTodayItemId !== null || dragState ? ' picking' : ''}${dragState ? ' dragging' : ''}`}
              onDragOver={handleLaneDragOver}
              onDrop={handleLaneDrop}
              ref={laneRef}
            >
              {Array.from({ length: (dayEnd - dayStart) / slotMinutes }, (_, index) => {
                const startMinute = dayStart + index * slotMinutes;
                return (
                  <button
                    aria-label={`在 ${formatMinute(startMinute)} 添加安排`}
                    className="schedule-time-slot"
                    key={startMinute}
                    onClick={() => void handleTimeSlotClick(startMinute)}
                    style={{
                      top: `${timelinePercent(startMinute)}%`,
                      height: `${(slotMinutes / (dayEnd - dayStart)) * 100}%`,
                    }}
                    type="button"
                  >
                    <span>{pendingTodayItemId !== null ? '放到这里' : '+'}</span>
                  </button>
                );
              })}
              {currentMinute !== null && currentMinute >= dayStart && currentMinute <= dayEnd && (
                <div className="schedule-now-line" style={{ top: `${timelinePercent(currentMinute)}%` }} />
              )}
              {positionedDayBlocks.map(({ block, columnCount, columnIndex }) => (
                <article
                  aria-label={`${block.title}，${formatMinute(block.start_minute)} 到 ${formatMinute(block.end_minute)}，${columnCount > 1 ? '时间冲突，' : ''}按 Enter 编辑，方向键每次移动 15 分钟，Shift 加方向键调整开始或结束时间，Delete 删除`}
                  aria-keyshortcuts="Enter Delete ArrowUp ArrowDown ArrowLeft ArrowRight Shift+ArrowUp Shift+ArrowDown Shift+ArrowLeft Shift+ArrowRight"
                  className={`schedule-block category-${block.category_key}${columnCount > 1 ? ' conflict' : ''}${dragState?.blockId === block.id ? ' is-dragging' : ''}`}
                  key={block.id}
                  onKeyDown={(event) => handleBlockKeyDown(event, block)}
                  onPointerDown={(event) => handleBlockPointerDown(event, block)}
                  style={positionedBlockTimelineStyle({ block, columnCount, columnIndex })}
                  tabIndex={0}
                >
                  {editingBlockId === block.id && editingBlockDraft ? (
                    <div className="schedule-block-editor">
                      <input
                        value={editingBlockDraft.title}
                        onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, title: event.target.value })}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleUpdateBlock();
                          if (event.key === 'Escape') { setEditingBlockId(null); setEditingBlockDraft(null); }
                        }}
                      />
                      <input type="time" value={formatMinute(editingBlockDraft.startMinute)} onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, startMinute: parseTime(event.target.value) })} />
                      <input type="time" value={formatMinute(editingBlockDraft.endMinute)} onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, endMinute: parseTime(event.target.value) })} />
                      <button disabled={saving} type="button" onClick={() => void handleUpdateBlock()}>保存</button>
                      <button type="button" onClick={() => { setEditingBlockId(null); setEditingBlockDraft(null); }}>取消</button>
                    </div>
                  ) : (
                    <>
                      <button
                        aria-label={`调整 ${block.title} 的开始时间`}
                        className="schedule-resize-handle is-start"
                        onPointerDown={(event) => handleResizePointerDown(event, block, 'resize-start')}
                        type="button"
                      />
                      <div onDoubleClick={() => beginEditBlock(block)}>
                        <span>{formatMinute(block.start_minute)}-{formatMinute(block.end_minute)} · {categoryLabel(block.category_key)}</span>
                        <strong>{block.title}</strong>
                        <small>{subjectName(subjects, block.subject_id)}</small>
                        {columnCount > 1 && <span className="schedule-conflict-badge">时间冲突，点击编辑解决</span>}
                      </div>
                      <div className="schedule-block-actions">
                        <button aria-label="开始专注" type="button" onClick={() => void handleStart(block)}><Play size={14} /></button>
                        <button
                          aria-label={`编辑 ${block.title}`}
                          style={{ minWidth: '56px', width: 'auto', padding: '0 8px' }}
                          type="button"
                          onClick={() => beginEditBlock(block)}
                        >
                          <PencilLine size={14} /> 编辑
                        </button>
                        <button aria-label="删除" type="button" onClick={() => void withSave(() => deleteScheduleBlock(block.id), '课表块已删除。')}><Trash2 size={14} /></button>
                      </div>
                      <button
                        aria-label={`调整 ${block.title} 的结束时间`}
                        className="schedule-resize-handle is-end"
                        onPointerDown={(event) => handleResizePointerDown(event, block, 'resize-end')}
                        type="button"
                      />
                    </>
                  )}
                </article>
              ))}
              {dragState && (
                <div
                  className={`schedule-drag-preview is-${dragState.mode}`}
                  style={rangeTimelineStyle(dragState.startMinute, dragState.endMinute)}
                >
                  <strong>{dragState.title}</strong>
                  <span>{formatMinute(dragState.startMinute)}-{formatMinute(dragState.endMinute)}</span>
                </div>
              )}
              {quickAddDraft && (
                <div
                  className={`schedule-quick-add${timelinePercent(quickAddDraft.startMinute) > 62 ? ' is-above' : ''}`}
                  style={{
                    top: `${timelinePercent(quickAddDraft.startMinute)}%`,
                  }}
                >
                  <div className="schedule-quick-add-head">
                    <strong>{formatMinute(quickAddDraft.startMinute)} 快速添加</strong>
                    <button type="button" onClick={() => { setQuickAddDraft(null); setQuickAddSourceTodayItemId(null); }}>×</button>
                  </div>
                  <select value={quickAddSourceTodayItemId ?? ''} onChange={(event) => handleQuickSourceChange(event.target.value)}>
                    <option value="">手动安排</option>
                    {(data?.today_items ?? []).map((item) => (
                      <option key={item.id} value={item.id}>{item.title}</option>
                    ))}
                  </select>
                  <input
                    disabled={quickAddSourceTodayItemId !== null}
                    placeholder="安排标题"
                    value={quickAddDraft.title}
                    onChange={(event) => setQuickAddDraft({ ...quickAddDraft, title: event.target.value })}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleQuickAddSave();
                      if (event.key === 'Escape') { setQuickAddDraft(null); setQuickAddSourceTodayItemId(null); }
                    }}
                  />
                  <div className="schedule-quick-add-row">
                    <input type="time" value={formatMinute(quickAddDraft.startMinute)} onChange={(event) => setQuickAddDraft({ ...quickAddDraft, startMinute: parseTime(event.target.value) })} />
                    <input type="time" value={formatMinute(quickAddDraft.endMinute)} onChange={(event) => setQuickAddDraft({ ...quickAddDraft, endMinute: parseTime(event.target.value) })} />
                  </div>
                  <select
                    disabled={quickAddSourceTodayItemId !== null}
                    value={quickAddDraft.subjectId ?? ''}
                    onChange={(event) => applyQuickSubject(event.target.value ? Number(event.target.value) : null)}
                  >
                    <option value="">未指定科目</option>
                    {subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.name}</option>)}
                  </select>
                  <button className="primary-button" disabled={saving || (quickAddSourceTodayItemId === null && !quickAddDraft.title.trim())} type="button" onClick={() => void handleQuickAddSave()}>
                    保存到课表
                  </button>
                </div>
              )}
              {!data?.day_blocks.length && !quickAddDraft && <div className="schedule-empty"><CalendarDays size={28} />点击时间格添加今天的安排。</div>}
            </div>
          </section>
        ) : (
          <section className="week-board soft-panel">
            {data?.week_days.map((day, index) => (
              <article className="week-day" key={day.date}>
                <header><strong>{weekdays[index]}</strong><span>{day.date.slice(5)} · {Math.round(day.planned_minutes / 60 * 10) / 10}h</span></header>
                <div className="week-blocks">
                  {day.blocks.map((block) => (
                    <button key={block.id} className={`week-block category-${block.category_key}`} type="button" onClick={() => { setSelectedDate(day.date); setView('day'); }}>
                      <span>{formatMinute(block.start_minute)}</span>{block.title}
                    </button>
                  ))}
                </div>
              </article>
            ))}
          </section>
        )}

        <aside className="template-rail soft-panel">
          <div className="panel-title compact-title">
            <div>
              <p className="eyebrow">Template</p>
              <h3>周模板</h3>
            </div>
            <Clock3 size={18} />
          </div>
          {data?.templates.length ? data.templates.map((template) => (
            <article className="template-row" key={template.id}>
              <div>
                <strong>{template.title}</strong>
                <span>{template.weekdays.map((day) => weekdays[day - 1]).join('、')} · {formatMinute(template.start_minute)}-{formatMinute(template.end_minute)}</span>
              </div>
              <div className="template-row-actions">
                <button aria-label={`编辑模板 ${template.title}`} type="button" onClick={() => beginEditTemplate(template)}><PencilLine size={14} />编辑</button>
                <button aria-label="删除模板" type="button" onClick={() => void handleDeleteTemplate(template.id)}><Trash2 size={14} /></button>
              </div>
            </article>
          )) : <div className="empty-state compact">还没有周模板。</div>}
        </aside>
      </div>
    </div>
    )
  );
}

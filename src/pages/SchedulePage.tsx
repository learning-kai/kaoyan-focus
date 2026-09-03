import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type {
  CSSProperties,
  DragEvent as ReactDragEvent,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from 'react';
import {
  CalendarDays,
  Check,
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
import { completeTodayPlanItem } from '../services/checklistApi';
import { getAppSettings } from '../services/settingsApi';
import { syncConfiguredStateChange } from '../services/syncApi';
import { FEISHU_SYNC_REFRESH_EVENT, syncFeishuBridge } from '../services/feishuApi';
import { CALDAV_SYNC_REFRESH_EVENT } from '../services/caldavApi';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import {
  createScheduleBlock,
  createScheduleBlockFromTodayItem,
  createScheduleTemplate,
  deleteScheduleBlock,
  deleteScheduleTemplate,
  getSchedulePageData,
  moveScheduleBlock,
  setScheduleBlockCompleted,
  startStudyModeFromScheduleBlock,
  updateScheduleBlock,
  updateScheduleTemplate,
} from '../services/scheduleApi';
import { listFocusSessionsInRange, listSubjects } from '../services/focusApi';
import type { AppSettings } from '../types/settings';
import type { FocusSession, Subject } from '../types/focus';
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
const timelineHourHeight = 80;
const timelineHeight = ((dayEnd - dayStart) / 60) * timelineHourHeight;
const timelineSpanMinutes = dayEnd - dayStart;
const focusBandFallbackColor = '#4fd0a1';
const minimumReadableBlockMinutes = 54;
const calendarDragActivationDistance = 8;
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
  const height = Math.max(
    (minimumReadableBlockMinutes / (dayEnd - dayStart)) * 100,
    ((visibleEnd - visibleStart) / (dayEnd - dayStart)) * 100,
  );
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

type FocusBand = {
  id: number;
  topPercent: number;
  heightPercent: number;
  color: string;
  subjectLabel: string;
  durationMinutes: number;
  startLabel: string;
  endLabel: string;
  running: boolean;
  paused: boolean;
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

type PendingBlockDragState = {
  block: ScheduleBlock;
  originClientX: number;
  originClientY: number;
  pointerId: number;
};

type ScheduleBlockDetail = {
  anchor: Pick<DOMRect, 'bottom' | 'left' | 'right' | 'top'>;
  block: ScheduleBlock;
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
    const visualEndMinute = Math.max(block.end_minute, block.start_minute + minimumReadableBlockMinutes);
    if (!activeGroup.length || block.start_minute < activeGroupEnd) {
      activeGroup.push(block);
      activeGroupEnd = Math.max(activeGroupEnd, visualEndMinute);
    } else {
      groups.push(activeGroup);
      activeGroup = [block];
      activeGroupEnd = visualEndMinute;
    }
  }
  if (activeGroup.length) groups.push(activeGroup);

  return groups.flatMap((group) => {
    const columnEnds: number[] = [];
    const assigned = group.map((block) => {
      const reusableColumn = columnEnds.findIndex((endMinute) => endMinute <= block.start_minute);
      const columnIndex = reusableColumn >= 0 ? reusableColumn : columnEnds.length;
      columnEnds[columnIndex] = Math.max(block.end_minute, block.start_minute + minimumReadableBlockMinutes);
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

function isScheduleBlockCompleted(
  block: ScheduleBlock,
  todayItems: Array<{ id: number; completed: boolean }>,
) {
  if (block.status === 'completed') {
    return true;
  }
  if (block.source_today_item_id == null) {
    return false;
  }
  return todayItems.some((item) => item.id === block.source_today_item_id && item.completed);
}

function scheduleBlockStatusClass(
  block: ScheduleBlock,
  todayItems: Array<{ id: number; completed: boolean }>,
) {
  const completed = isScheduleBlockCompleted(block, todayItems);
  const running = block.status === 'running';
  return `${completed ? ' is-completed' : ''}${running ? ' is-running' : ''}`;
}

function scheduleBlockStatusLabel(
  block: ScheduleBlock,
  todayItems: Array<{ id: number; completed: boolean }>,
) {
  if (isScheduleBlockCompleted(block, todayItems)) return '已完成';
  if (block.status === 'running') return '进行中';
  return null;
}

function formatDurationLabel(minutes: number) {
  const total = Math.max(0, Math.round(minutes));
  const hours = Math.floor(total / 60);
  const rest = total % 60;
  if (hours <= 0) return `${rest} 分钟`;
  return rest === 0 ? `${hours} 小时` : `${hours} 小时 ${rest} 分钟`;
}

function dateKeyToParts(dateKey: string) {
  const [year, month, day] = dateKey.split('-').map(Number);
  if (!year || !month || !day) return null;
  return { year, month, day };
}

/**
 * 把一段真实的专注记录投影到时间轴上。
 * 用时间戳差值而不是“当日第几分钟”计算，跨零点的长记录也能正确裁剪到可视范围。
 */
function projectSessionToLane(
  session: FocusSession,
  windowStartTs: number,
  windowEndTs: number,
  nowTs: number,
): { startMinute: number; endMinute: number; fullMinutes: number } | null {
  const startedTs = new Date(session.started_at).getTime();
  if (!Number.isFinite(startedTs)) return null;
  const running = session.status === 'running';
  // 进行中且被暂停：色带冻结在暂停那一刻，暂停之后的区间保持空白（不再计入专注）。
  const pausedAtTs = session.paused_at ? new Date(session.paused_at).getTime() : Number.NaN;
  const endedTs = running
    ? Number.isFinite(pausedAtTs)
      ? pausedAtTs
      : nowTs
    : session.ended_at
      ? new Date(session.ended_at).getTime()
      : startedTs + Math.max(0, session.actual_seconds) * 1000;

  if (endedTs <= windowStartTs || startedTs >= windowEndTs) return null;

  const startMinute = (Math.max(startedTs, windowStartTs) - windowStartTs) / 60_000;
  const endMinute = (Math.min(endedTs, windowEndTs) - windowStartTs) / 60_000;
  const fullMinutes = Math.max(0, (endedTs - startedTs) / 60_000);
  return { startMinute, endMinute, fullMinutes };
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
  const { confirm, confirmDialog } = useConfirmDialog();
  const [data, setData] = useState<SchedulePageData | null>(null);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [selectedDate, setSelectedDate] = useState(formatDateKey());
  const [dateDraft, setDateDraft] = useState(formatDateKey());
  const [nowMinute, setNowMinute] = useState(() => currentMinuteOfDay());
  const [nowTimestamp, setNowTimestamp] = useState(() => Date.now());
  const [focusSessions, setFocusSessions] = useState<FocusSession[]>([]);
  const [focusBandsVisible, setFocusBandsVisible] = useState(true);
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
  const [pendingBlockDrag, setPendingBlockDrag] = useState<PendingBlockDragState | null>(null);
  const [selectedBlockDetail, setSelectedBlockDetail] = useState<ScheduleBlockDetail | null>(null);
  const refreshTokenRef = useRef(0);
  const focusTokenRef = useRef(0);
  const laneRef = useRef<HTMLDivElement | null>(null);
  const timelineScrollRef = useRef<HTMLDivElement | null>(null);
  const detailRef = useRef<HTMLElement | null>(null);
  const detailTriggerRef = useRef<HTMLElement | null>(null);
  const dragStateRef = useRef<CalendarDragState | null>(null);
  const pendingBlockDragRef = useRef<PendingBlockDragState | null>(null);
  const suppressBlockClickRef = useRef(false);
  const blockedTodayItemDragRef = useRef<number | null>(null);
  const dragFrameRef = useRef<number | null>(null);
  const pendingDragClientYRef = useRef<number | null>(null);
  const dragEffectKey = dragState && dragState.mode !== 'create' ? `${dragState.mode}:${dragState.blockId ?? ''}` : null;
  const pointerGestureKey = pendingBlockDrag
    ? `pending:${pendingBlockDrag.pointerId}:${pendingBlockDrag.block.id}`
    : dragEffectKey;

  useEffect(() => {
    void initialize();
  }, []);

  useEffect(() => {
    void refresh(selectedDate);
    void loadFocusSessions(selectedDate);
    setDateDraft(selectedDate);
    setBlockDraft((draft) => ({ ...draft, scheduleDate: selectedDate }));
    setSelectedBlockDetail(null);
  }, [selectedDate]);

  // 看今天时每分钟重取一次，让进行中的专注和刚开始的专注都能及时出现。
  useEffect(() => {
    if (selectedDate !== formatDateKey()) return;
    void loadFocusSessions(selectedDate);
  }, [selectedDate, nowMinute]);

  useEffect(() => {
    const handleCalendarRefresh = () => {
      void refresh(selectedDate);
    };
    window.addEventListener(FEISHU_SYNC_REFRESH_EVENT, handleCalendarRefresh);
    window.addEventListener(CALDAV_SYNC_REFRESH_EVENT, handleCalendarRefresh);
    return () => {
      window.removeEventListener(FEISHU_SYNC_REFRESH_EVENT, handleCalendarRefresh);
      window.removeEventListener(CALDAV_SYNC_REFRESH_EVENT, handleCalendarRefresh);
    };
  }, [selectedDate]);

  useEffect(() => {
    let intervalId: number | undefined;
    const syncNow = () => {
      setNowMinute(currentMinuteOfDay());
      setNowTimestamp(Date.now());
    };
    const now = new Date();
    const msToNextMinute = (60 - now.getSeconds()) * 1000 - now.getMilliseconds() + 250;
    const timeoutId = window.setTimeout(() => {
      syncNow();
      intervalId = window.setInterval(syncNow, 60_000);
    }, msToNextMinute);
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') syncNow();
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      window.clearTimeout(timeoutId);
      if (intervalId !== undefined) window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  useEffect(() => {
    if (!data || view !== 'day') return;
    const scrollContainer = timelineScrollRef.current;
    if (!scrollContainer) return;
    const nowMinute = currentMinuteOfDay();
    const firstBlockMinute = data.day_blocks.reduce<number | null>(
      (first, block) => first === null || block.start_minute < first ? block.start_minute : first,
      null,
    );
    const targetMinute = selectedDate === data.today_date
      ? nowMinute - 2 * 60
      : firstBlockMinute === null ? 7 * 60 : firstBlockMinute - 60;
    const scrollTop = Math.max(0, ((targetMinute - dayStart) / 60) * timelineHourHeight);
    const frame = window.requestAnimationFrame(() => {
      scrollContainer.scrollTop = scrollTop;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [data, selectedDate, view]);

  useEffect(() => {
    if (!selectedBlockDetail) return;
    const closeDetail = () => setSelectedBlockDetail(null);
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!detailRef.current?.contains(target) && !detailTriggerRef.current?.contains(target)) {
        closeDetail();
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeDetail();
    };
    const scrollContainer = timelineScrollRef.current;
    const focusFrame = window.requestAnimationFrame(() => detailRef.current?.focus());
    window.addEventListener('resize', closeDetail);
    window.addEventListener('keydown', handleKeyDown);
    document.addEventListener('pointerdown', handlePointerDown);
    scrollContainer?.addEventListener('scroll', closeDetail, { passive: true });
    return () => {
      window.removeEventListener('resize', closeDetail);
      window.removeEventListener('keydown', handleKeyDown);
      document.removeEventListener('pointerdown', handlePointerDown);
      scrollContainer?.removeEventListener('scroll', closeDetail);
      window.cancelAnimationFrame(focusFrame);
      if (detailTriggerRef.current?.isConnected) detailTriggerRef.current.focus();
    };
  }, [selectedBlockDetail]);

  function setCalendarDragState(next: CalendarDragState | null) {
    dragStateRef.current = next;
    setDragState(next);
  }

  function setPendingBlockDragState(next: PendingBlockDragState | null) {
    pendingBlockDragRef.current = next;
    setPendingBlockDrag(next);
  }

  useEffect(() => {
    const active = dragStateRef.current;
    const pending = pendingBlockDragRef.current;
    if ((!active || active.mode === 'create') && !pending) return;

    function handlePointerMove(event: PointerEvent) {
      const pendingDrag = pendingBlockDragRef.current;
      if (pendingDrag) {
        if (event.pointerId !== pendingDrag.pointerId) return;
        const distance = Math.hypot(
          event.clientX - pendingDrag.originClientX,
          event.clientY - pendingDrag.originClientY,
        );
        if (distance < calendarDragActivationDistance) return;

        event.preventDefault();
        suppressBlockClickRef.current = true;
        setPendingBlockDragState(null);
        startBlockDrag(pendingDrag.block, 'move', pendingDrag.originClientY);
        updateDragPreview(event.clientY);
        return;
      }

      const current = dragStateRef.current;
      if (!current || current.mode === 'create') return;
      event.preventDefault();
      scheduleDragPreview(event.clientY);
    }

    function handlePointerUp(event: PointerEvent) {
      const pendingDrag = pendingBlockDragRef.current;
      if (pendingDrag) {
        if (event.pointerId !== pendingDrag.pointerId) return;
        setPendingBlockDragState(null);
        return;
      }

      const current = dragStateRef.current;
      if (!current || current.mode === 'create') return;
      flushScheduledDragPreview();
      void commitDrag(current);
    }

    function handlePointerCancel(event: PointerEvent) {
      const pendingDrag = pendingBlockDragRef.current;
      if (pendingDrag && event.pointerId === pendingDrag.pointerId) {
        setPendingBlockDragState(null);
      }
      if (dragStateRef.current?.mode !== 'create') {
        cancelScheduledDragPreview();
        setCalendarDragState(null);
      }
    }

    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp);
    window.addEventListener('pointercancel', handlePointerCancel);
    return () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerUp);
      window.removeEventListener('pointercancel', handlePointerCancel);
      cancelScheduledDragPreview();
    };
  }, [pointerGestureKey]);

  const positionedDayBlocks = useMemo(
    () => layoutScheduleBlocks(data?.day_blocks ?? []),
    [data?.day_blocks],
  );

  const focusBands = useMemo<FocusBand[]>(() => {
    const parts = dateKeyToParts(selectedDate);
    if (!parts) return [];
    const windowStartTs = new Date(parts.year, parts.month - 1, parts.day, 6, 0, 0, 0).getTime();
    const windowEndTs = new Date(parts.year, parts.month - 1, parts.day + 1, 0, 0, 0, 0).getTime();
    if (windowEndTs <= windowStartTs) return [];

    return focusSessions
      .map((session) => {
        const projected = projectSessionToLane(session, windowStartTs, windowEndTs, nowTimestamp);
        if (!projected) return null;
        const visibleMinutes = Math.max(0, projected.endMinute - projected.startMinute);
        // 极短的专注也要留下可见痕迹，否则用户会以为没记录上。
        const renderEndMinute = Math.max(projected.endMinute, projected.startMinute + 4);
        const subject = subjects.find((item) => item.id === session.subject_id);
        return {
          id: session.id,
          topPercent: (projected.startMinute / timelineSpanMinutes) * 100,
          heightPercent: ((renderEndMinute - projected.startMinute) / timelineSpanMinutes) * 100,
          color: subject?.color?.trim() || focusBandFallbackColor,
          subjectLabel: subject?.name ?? '未指定科目',
          durationMinutes: visibleMinutes,
          startLabel: formatMinute(projected.startMinute + dayStart),
          endLabel: formatMinute(projected.endMinute + dayStart),
          running: session.status === 'running' && session.paused_at == null,
          paused: session.status === 'running' && session.paused_at != null,
        } satisfies FocusBand;
      })
      .filter((band): band is FocusBand => band !== null);
  }, [focusSessions, nowTimestamp, selectedDate, subjects]);

  const focusTotalMinutes = useMemo(
    () => focusBands.reduce((total, band) => total + band.durationMinutes, 0),
    [focusBands],
  );

  const focusRunningBand = useMemo(
    () => focusBands.find((band) => band.running) ?? null,
    [focusBands],
  );

  const currentMinute = useMemo(() => {
    if (selectedDate !== formatDateKey()) return null;
    return nowMinute;
  }, [nowMinute, selectedDate]);

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

  /** 读取某一天实际发生的专注记录，用于在时间轴上铺底色。 */
  async function loadFocusSessions(date = selectedDate) {
    const parts = dateKeyToParts(date);
    if (!parts) {
      setFocusSessions([]);
      return;
    }
    const token = focusTokenRef.current + 1;
    focusTokenRef.current = token;
    const dayStartTs = new Date(parts.year, parts.month - 1, parts.day, 0, 0, 0, 0);
    const nextDayTs = new Date(parts.year, parts.month - 1, parts.day + 1, 0, 0, 0, 0);
    try {
      const sessions = await listFocusSessionsInRange(dayStartTs.toISOString(), nextDayTs.toISOString());
      if (focusTokenRef.current === token) {
        setFocusSessions(sessions);
      }
    } catch {
      // 专注底色只是辅助信息，读不到就静默降级为空。
      if (focusTokenRef.current === token) {
        setFocusSessions([]);
      }
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
      setMessage(feishuResult.status === 'synced' ? '日历已同步到飞书日历。' : '日历已刷新，本地修改已自动保存。');
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
    }, state.mode === 'create' ? '今日任务已生成日程。' : '日程时间已更新。');
  }

  function isInteractiveElement(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    return Boolean(target.closest('button, input, select, textarea, a, .schedule-resize-handle'));
  }

  function handleTodayItemDragStart(event: ReactDragEvent<HTMLElement>, itemId: number, title: string) {
    if (blockedTodayItemDragRef.current === itemId) {
      blockedTodayItemDragRef.current = null;
      event.preventDefault();
      return;
    }
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
    setPendingBlockDragState({
      block,
      originClientX: event.clientX,
      originClientY: event.clientY,
      pointerId: event.pointerId,
    });
  }

  function openBlockDetail(block: ScheduleBlock, trigger: HTMLElement) {
    detailTriggerRef.current = trigger;
    const { bottom, left, right, top } = trigger.getBoundingClientRect();
    setSelectedBlockDetail({ block, anchor: { bottom, left, right, top } });
  }

  function handleBlockClick(event: React.MouseEvent<HTMLElement>, block: ScheduleBlock) {
    if (suppressBlockClickRef.current) {
      suppressBlockClickRef.current = false;
      return;
    }
    if (editingBlockId === block.id || isInteractiveElement(event.target)) return;
    if (pendingBlockDragRef.current || dragStateRef.current) return;
    openBlockDetail(block, event.currentTarget);
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
      openBlockDetail(block, event.currentTarget);
      return;
    }

    if (event.key === 'Delete') {
      event.preventDefault();
      event.stopPropagation();
      void withSave(() => deleteScheduleBlock(block.id), '日程已删除。');
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
      '日程时间已更新。',
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
      setError('周重复需要先填写标题。');
      return false;
    }
    if (draft.endMinute <= draft.startMinute) {
      setMessage(null);
      setError('周重复的结束时间必须晚于开始时间。');
      return false;
    }
    if (draft.weekdays.length === 0) {
      setMessage(null);
      setError('周重复至少需要选择一个生效日期。');
      return false;
    }
    return true;
  }

  async function handleCreateBlock() {
    if (!validateBlockDraft(blockDraft, '日程')) return;
    await withSave(async () => {
      await createScheduleBlock(blockDraft);
      setBlockDraft(emptyBlockDraft(selectedDate));
      setShowBlockComposer(false);
    }, '日程已添加。');
  }

  async function handleCompleteTodayItem(itemId: number, completed: boolean) {
    const item = data?.today_items.find((candidate) => candidate.id === itemId);
    if (!item) return;

    const nextCompleted = !completed;
    let syncSourceCompletion = false;
    if (nextCompleted && item.source_task_id !== null) {
      syncSourceCompletion = await confirm({
        cancelLabel: '只完成今日任务',
        confirmLabel: '同步完成',
        message: '这条今日任务来自清单待办。同步完成会把源待办也移入已完成；只完成今日任务则保留源待办。',
        title: '同步完成源待办？',
      });
    }

    await withSave(async () => {
      await completeTodayPlanItem(itemId, nextCompleted, syncSourceCompletion);
    }, nextCompleted ? '今日任务已完成。' : '今日任务已恢复为未完成。');
  }

  async function handleCompleteScheduleBlock(block: ScheduleBlock, completed: boolean) {
    if (block.source_today_item_id !== null) {
      await handleCompleteTodayItem(block.source_today_item_id, completed);
      return;
    }

    await withSave(async () => {
      await setScheduleBlockCompleted(block.id, !completed);
    }, completed ? '日程已恢复为未完成。' : '日程已完成。');
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
    }, '今日任务已生成日程。');
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
    }, quickAddSourceTodayItemId !== null ? '今日任务已生成日程。' : '日程已添加。');
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
    }, editingTemplateId !== null ? '周重复已更新。' : '周重复已保存。');
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
    setSelectedBlockDetail(null);
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
    if (!validateBlockDraft(editingBlockDraft, '日程')) return;
    await withSave(async () => {
      await updateScheduleBlock(editingBlockId, editingBlockDraft);
      setEditingBlockId(null);
      setEditingBlockDraft(null);
    }, '日程已更新。');
  }

  async function handleStart(block: ScheduleBlock) {
    setSelectedBlockDetail(null);
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
    }, '已从日程开始专注。', 'focus_state_change');
  }

  const blockDetail = selectedBlockDetail && typeof document !== 'undefined' ? (() => {
    const { block, anchor } = selectedBlockDetail;
    const blockCompleted = isScheduleBlockCompleted(block, data?.today_items ?? []);
    const detailWidth = 320;
    const left = Math.max(12, Math.min(window.innerWidth - detailWidth - 12, anchor.left));
    const showAbove = anchor.bottom + 260 > window.innerHeight && anchor.top > 272;
    const top = showAbove ? Math.max(12, anchor.top - 12) : Math.min(window.innerHeight - 12, anchor.bottom + 8);
    return createPortal(
      <section
        aria-label={`${block.title} 日程详情`}
        className={`schedule-block-detail${showAbove ? ' is-above' : ''}`}
        ref={detailRef}
        role="dialog"
        style={{ left, top }}
        tabIndex={-1}
      >
        <div className="schedule-block-detail-head">
          <div>
            <span>{block.schedule_date} · {formatMinute(block.start_minute)}-{formatMinute(block.end_minute)}</span>
            <h3>{block.title}</h3>
          </div>
          <button aria-label="关闭日程详情" className="icon-button" type="button" onClick={() => setSelectedBlockDetail(null)}>×</button>
        </div>
        <div className="schedule-block-detail-meta">
          <span>{categoryLabel(block.category_key)}</span>
          <span>{subjectName(subjects, block.subject_id)}</span>
          <span>{blockCompleted ? '已完成' : block.status === 'running' ? '进行中' : '待开始'}</span>
        </div>
        {block.note && <p className="schedule-block-detail-note">{block.note}</p>}
        <div className="schedule-block-detail-actions">
          <button disabled={saving || block.status === 'running'} type="button" onClick={() => void handleCompleteScheduleBlock(block, blockCompleted)}>
            <Check size={15} />{blockCompleted ? '恢复未完成' : '标记完成'}
          </button>
          <button type="button" onClick={() => void handleStart(block)}><Play size={15} />开始专注</button>
          <button type="button" onClick={() => beginEditBlock(block)}><PencilLine size={15} />编辑</button>
          <button className="danger" type="button" onClick={() => {
            setSelectedBlockDetail(null);
            void withSave(() => deleteScheduleBlock(block.id), '日程已删除。');
          }}><Trash2 size={15} />删除</button>
        </div>
      </section>,
      document.body,
    );
  })() : null;

  return (
    loadingSchedule && data === null ? (
      <section className="page-shell schedule-page">
        <div className="empty-state">
          <strong>正在载入日历</strong>
          <p>正在读取今日安排、周重复和同步状态。</p>
        </div>
      </section>
    ) : (
    <div className="schedule-page">
      <section className="schedule-hero">
        <div>
          <p className="eyebrow">日程安排</p>
          <h2>今日日历</h2>
          <p>把今日任务和手动安排放进一天的时间轴。新增、拖拽、删除会自动保存；需要时可手动同步日历/云端。</p>
        </div>
        <div className="schedule-actions">
          <button className="primary-button" disabled={saving || loadingSchedule} type="button" onClick={() => void handleSyncSchedule()}>
            <RefreshCw size={16} /> 同步日历
          </button>
          <button className="ghost-button" type="button" onClick={() => void refresh()}>
            <RefreshCw size={16} /> 刷新
          </button>
          <button className="primary-button" type="button" onClick={() => setShowBlockComposer((value) => !value)}>
            <Plus size={16} /> 日程
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

      {confirmDialog}
      {blockDetail}

      <section className="schedule-toolbar soft-panel">
        <div className="segmented-control">
          <button className={view === 'day' ? 'active' : ''} type="button" onClick={() => setView('day')}>今日日历</button>
          <button className={view === 'week' ? 'active' : ''} type="button" onClick={() => setView('week')}>本周</button>
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
          <CopyPlus size={16} /> 周重复
        </button>
      </section>

      {loadingSchedule && <div className="schedule-loading-hint">正在更新日历...</div>}

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
          <label className="schedule-time-field">
            <span>开始时间</span>
            <input
              aria-label="周重复开始时间"
              step={900}
              type="time"
              value={formatMinute(templateDraft.startMinute)}
              onChange={(event) => setTemplateDraft({ ...templateDraft, startMinute: parseTime(event.target.value) })}
            />
          </label>
          <label className="schedule-time-field">
            <span>结束时间</span>
            <input
              aria-label="周重复结束时间"
              step={900}
              type="time"
              value={formatMinute(templateDraft.endMinute)}
              onChange={(event) => setTemplateDraft({ ...templateDraft, endMinute: parseTime(event.target.value) })}
            />
          </label>
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
                className={`schedule-task-row${picking ? ' picking' : ''}${item.completed ? ' is-completed' : ''}`}
                draggable={!saving}
                key={item.id}
                onDragEnd={handleTodayItemDragEnd}
                onDragStart={(event) => handleTodayItemDragStart(event, item.id, item.title)}
              >
                <div className="schedule-task-main">
                  <button
                    aria-label={item.completed ? '恢复为未完成' : '标记完成'}
                    className={item.completed ? 'small-action icon-action enabled' : 'small-action icon-action'}
                    disabled={saving}
                    title={item.completed ? '恢复为未完成' : '标记完成'}
                    type="button"
                    onPointerCancel={() => { blockedTodayItemDragRef.current = null; }}
                    onPointerDown={(event) => {
                      event.stopPropagation();
                      blockedTodayItemDragRef.current = item.id;
                    }}
                    onPointerUp={() => { blockedTodayItemDragRef.current = null; }}
                    onClick={(event) => {
                      event.stopPropagation();
                      void handleCompleteTodayItem(item.id, item.completed);
                    }}
                  >
                    <Check size={13} />
                  </button>
                  <div className="schedule-task-copy">
                    <strong>{item.title}</strong>
                    <span>{subjectName(subjects, item.subject_id)}{item.due_date ? ` / ${item.due_date}` : ''}</span>
                  </div>
                </div>
                <div className="schedule-task-actions">
                  <button disabled={saving || item.completed} type="button" onClick={() => void handleAddTodayItem(item.id)}>
                    {picking ? '取消' : '选时间'}
                  </button>
                  <div className="schedule-quick-slots" aria-label={`${item.title} 快捷安排`}>
                    {quickScheduleSlots.map((slot) => (
                      <button
                        aria-label={`安排 ${item.title} 到${slot.label} ${formatMinute(slot.minute)}`}
                        disabled={saving || item.completed}
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
            <div className="schedule-timeline-head">
              <div className="schedule-focus-summary">
                <span aria-hidden="true" className="schedule-focus-swatch" />
                <strong>{formatDurationLabel(focusTotalMinutes)}</strong>
                <span>
                  {focusRunningBand
                    ? `专注中 · 共 ${focusBands.length} 段`
                    : focusBands.length
                      ? `当天专注 · ${focusBands.length} 段`
                      : '当天还没有专注记录'}
                </span>
              </div>
              <button
                aria-pressed={focusBandsVisible}
                className={`schedule-focus-toggle${focusBandsVisible ? ' is-on' : ''}`}
                disabled={!focusBands.length}
                onClick={() => setFocusBandsVisible((value) => !value)}
                type="button"
              >
                {focusBandsVisible ? '隐藏专注底色' : '显示专注底色'}
              </button>
            </div>
            <div className="schedule-timeline-scroll" ref={timelineScrollRef}>
              <div className="schedule-time-column" style={{ height: timelineHeight }}>
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
                style={{ height: timelineHeight }}
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
              {focusBandsVisible && focusBands.length > 0 && (
                <div className="schedule-focus-bands" aria-hidden="true">
                  {focusBands.map((band) => (
                    <div
                      className={`schedule-focus-band${band.running ? ' is-running' : ''}${band.paused ? ' is-paused' : ''}`}
                      key={band.id}
                      style={{
                        top: `${band.topPercent}%`,
                        height: `${band.heightPercent}%`,
                        '--focus-band-color': band.color,
                      } as CSSProperties}
                      title={`${band.subjectLabel} ${band.startLabel}-${band.endLabel} 专注 ${formatDurationLabel(band.durationMinutes)}${band.running ? '（进行中）' : ''}${band.paused ? '（已暂停）' : ''}`}
                    >
                      <i className="schedule-focus-band-fill" />
                      {band.paused && <i className="schedule-focus-band-pause-mark" aria-hidden="true" />}
                      {band.heightPercent >= 1.4 && (
                        <span className="schedule-focus-band-label">
                          {band.subjectLabel} · {formatDurationLabel(band.durationMinutes)}
                          {band.running ? ' · 进行中' : ''}
                          {band.paused ? ' · 已暂停' : ''}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {currentMinute !== null && currentMinute >= dayStart && currentMinute <= dayEnd && (
                <div className="schedule-now-line" style={{ top: `${timelinePercent(currentMinute)}%` }} />
              )}
              {positionedDayBlocks.map(({ block, columnCount, columnIndex }) => {
                const blockCompleted = isScheduleBlockCompleted(block, data?.today_items ?? []);
                const statusLabel = scheduleBlockStatusLabel(block, data?.today_items ?? []);
                const compact = block.end_minute - block.start_minute < minimumReadableBlockMinutes;
                return (
                <article
                  aria-label={`${block.title}，${formatMinute(block.start_minute)} 到 ${formatMinute(block.end_minute)}，${statusLabel ? `${statusLabel}，` : ''}${block.has_conflict ? '时间冲突，' : ''}单击或按 Enter 打开详情，方向键每次移动 15 分钟，Shift 加方向键调整开始或结束时间，Delete 删除`}
                  aria-keyshortcuts="Enter Delete ArrowUp ArrowDown ArrowLeft ArrowRight Shift+ArrowUp Shift+ArrowDown Shift+ArrowLeft Shift+ArrowRight"
                  className={`schedule-block category-${block.category_key}${compact ? ' is-compact' : ''}${editingBlockId === block.id ? ' is-editing' : ''}${block.has_conflict ? ' conflict' : ''}${scheduleBlockStatusClass(block, data?.today_items ?? [])}${dragState?.blockId === block.id ? ' is-dragging' : ''}`}
                  key={block.id}
                  onClick={(event) => handleBlockClick(event, block)}
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
                      <div className="schedule-block-content">
                        <span>{formatMinute(block.start_minute)}-{formatMinute(block.end_minute)} · {categoryLabel(block.category_key)}{statusLabel ? ` · ${statusLabel}` : ''}</span>
                        <strong>{block.title}</strong>
                        <small>{subjectName(subjects, block.subject_id)}</small>
                        {blockCompleted && <span className="schedule-completed-badge">✓ 完成</span>}
                        {block.has_conflict && <span className="schedule-conflict-badge">时间冲突，点击详情解决</span>}
                      </div>
                      <div className="schedule-block-actions">
                        <button
                          aria-label={blockCompleted ? '恢复为未完成' : '标记完成'}
                          className={blockCompleted ? 'is-complete-toggle enabled' : 'is-complete-toggle'}
                          disabled={saving || block.status === 'running'}
                          title={block.status === 'running' ? '进行中的日程会在学习结束后自动完成' : blockCompleted ? '恢复为未完成' : '标记完成'}
                          type="button"
                          onPointerDown={(event) => event.stopPropagation()}
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleCompleteScheduleBlock(block, blockCompleted);
                          }}
                        >
                          <Check size={14} />
                        </button>
                        <button
                          aria-label={`删除 ${block.title}`}
                          className="is-delete-action"
                          title="删除日程"
                          type="button"
                          onPointerDown={(event) => event.stopPropagation()}
                          onClick={(event) => {
                            event.stopPropagation();
                            void withSave(() => deleteScheduleBlock(block.id), '日程已删除。');
                          }}
                        >
                          <Trash2 size={14} />
                        </button>
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
                );
              })}
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
                    {(data?.today_items ?? []).filter((item) => !item.completed).map((item) => (
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
                    保存到日历
                  </button>
                </div>
              )}
                {!data?.day_blocks.length && !quickAddDraft && <div className="schedule-empty"><CalendarDays size={28} />点击时间格添加今天的安排。</div>}
              </div>
            </div>
          </section>
        ) : (
          <section className="week-board soft-panel">
            {data?.week_days.map((day, index) => (
              <article className="week-day" key={day.date}>
                <header><strong>{weekdays[index]}</strong><span>{day.date.slice(5)} · {Math.round(day.planned_minutes / 60 * 10) / 10}h</span></header>
                <div className="week-blocks">
                  {day.blocks.map((block) => {
                    const blockCompleted = isScheduleBlockCompleted(block, data?.today_items ?? []);
                    const statusLabel = scheduleBlockStatusLabel(block, data?.today_items ?? []);
                    return (
                      <button
                        aria-label={`${block.title}，${formatMinute(block.start_minute)}${statusLabel ? `，${statusLabel}` : ''}`}
                        className={`week-block category-${block.category_key}${scheduleBlockStatusClass(block, data?.today_items ?? [])}`}
                        key={block.id}
                        type="button"
                        onClick={() => { setSelectedDate(day.date); setView('day'); }}
                      >
                        <span>{formatMinute(block.start_minute)}{blockCompleted ? ' · 完成' : ''}</span>
                        {block.title}
                        {blockCompleted && <span aria-hidden="true">✓</span>}
                      </button>
                    );
                  })}
                </div>
              </article>
            ))}
          </section>
        )}

        <aside className="template-rail soft-panel">
          <div className="panel-title compact-title">
            <div>
              <p className="eyebrow">Template</p>
              <h3>周重复</h3>
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
          )) : <div className="empty-state compact">还没有周重复。</div>}
        </aside>
      </div>
    </div>
    )
  );
}

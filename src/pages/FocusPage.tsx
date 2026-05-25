import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { closestCenter, DndContext, PointerSensor, useSensor, useSensors, type DragEndEvent } from '@dnd-kit/core';
import { arrayMove } from '@dnd-kit/sortable';
import { BellRing, BookOpen, CalendarClock, CheckCircle2, ClipboardList, Coffee, Gauge, Leaf, Pause, Play, ShieldCheck, Square, Timer } from 'lucide-react';
import ScheduleDrawer from '../components/ScheduleDrawer';
import TodayPlanDrawer from '../components/TodayPlanDrawer';
import { completeTodayPlanItem, createTodayPlanItem, deleteTodayPlanItem, getChecklistPageData, reorderTodayPlanItems, updateTodayPlanItem } from '../services/checklistApi';
import { confirmStudyBreak, getFocusStatsSummary, getStudyModeState, listFocusSessions, listSubjects, pauseStudyMode, resetStudyMode, resumeStudyMode, startStudyMode, updateStudyModeSubject } from '../services/focusApi';
import { notifyStudyReminder } from '../services/alertApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { createScheduleBlock, deleteScheduleBlock, getSchedulePageData, startStudyModeFromScheduleBlock } from '../services/scheduleApi';
import { FEISHU_SYNC_REFRESH_EVENT, STUDY_SYNC_STATE_CHANGED_EVENT, syncConfiguredStateChange, getAppSettings } from '../services/settingsApi';
import { setStudyFullscreen } from '../services/systemApi';
import type { ChecklistPageData, TodayPlanItem, TodayPlanItemDraft } from '../types/checklist';
import type { FocusMode, FocusSession, FocusStatsSummary, StudyModePhase, StudyModeState, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';
import type { ScheduleBlock, ScheduleBlockDraft, SchedulePageData } from '../types/schedule';

const studyPresetMinutes = [60, 120, 180, 240];
const focusPresetMinutes = [25, 45, 60, 90];
const breakPresetMinutes = [5, 10, 15, 20];
const longBreakPresetMinutes = [10, 15, 20, 30];
const longBreakIntervalPresets = [2, 3, 4, 6];
const FOCUS_TODAY_CONTAINER_ID = 'focus-today-container';
const emptyTodayDraft: TodayPlanItemDraft = { title: '', note: '', dueDate: '', subjectId: null };
const emptyScheduleDraft = (date: string): ScheduleBlockDraft => ({
  scheduleDate: date,
  title: '',
  note: '',
  categoryKey: 'general',
  subjectId: null,
  sourceTodayItemId: null,
  startMinute: 8 * 60,
  endMinute: 9 * 60,
});

const idleStudyState: StudyModeState = {
  id: null,
  phase: 'idle',
  status: 'idle',
  mode: 'normal',
  subject_id: null,
  planned_seconds: 0,
  focus_seconds: 0,
  break_seconds: 0,
  long_break_seconds: 0,
  long_break_interval: 4,
  effective_break_seconds: 0,
  break_kind: 'short',
  cycle_index: 0,
  started_at: null,
  phase_started_at: null,
  paused_at: null,
  ended_at: null,
  current_session: null,
  study_elapsed_seconds: 0,
  study_remaining_seconds: 0,
  phase_elapsed_seconds: 0,
  phase_remaining_seconds: 0,
  focus_enforcement_active: false,
  whitelist_enabled: true,
  is_paused: false,
};

const phaseLabel: Record<StudyModePhase, string> = {
  idle: '待开始',
  focus: '专注中',
  awaiting_break: '等待休息确认',
  break: '休息中',
  finished: '已完成',
  emergency_exited: '已退出',
};

let reminderBaselineKey: string | null = null;

function formatSeconds(totalSeconds: number) {
  const safeSeconds = Math.max(Math.floor(totalSeconds), 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60).toString().padStart(2, '0');
  const seconds = Math.floor(safeSeconds % 60).toString().padStart(2, '0');
  return hours > 0 ? hours + ':' + minutes + ':' + seconds : minutes + ':' + seconds;
}

function formatDuration(seconds: number) {
  if (seconds <= 0) return '0 分钟';
  if (seconds < 3600) return Math.round(seconds / 60) + ' 分钟';
  const hours = seconds / 3600;
  return (Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)) + ' 小时';
}

function formatDateTime(value: string | null) {
  if (!value) return '暂无';
  return new Date(value).toLocaleString();
}

function formatTimeOnly(value: string | null) {
  if (!value) return '暂无';
  return new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatSessionTimeRange(session: FocusSession) {
  if (!session.ended_at) {
    return `${formatDateTime(session.started_at)} - 未记录结束`;
  }
  const start = new Date(session.started_at);
  const end = new Date(session.ended_at);
  return start.toDateString() === end.toDateString()
    ? `${formatDateTime(session.started_at)} - ${formatTimeOnly(session.ended_at)}`
    : `${formatDateTime(session.started_at)} - ${formatDateTime(session.ended_at)}`;
}

function todayString() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function sessionStatusLabel(status: string) {
  const labels: Record<string, string> = { running: '进行中', finished: '已完成', interrupted: '已中断', emergency_exited: '已退出' };
  return labels[status] ?? status;
}

export default function FocusPage() {
  const [studyMinutes, setStudyMinutes] = useState(120);
  const [focusMinutes, setFocusMinutes] = useState(25);
  const [breakMinutes, setBreakMinutes] = useState(5);
  const [longBreakMinutes, setLongBreakMinutes] = useState(15);
  const [longBreakInterval, setLongBreakInterval] = useState(4);
  const [mode, setMode] = useState<FocusMode>('normal');
  const [normalWhitelistEnabled, setNormalWhitelistEnabled] = useState(true);
  const [studyState, setStudyState] = useState<StudyModeState>(idleStudyState);
  const [history, setHistory] = useState<FocusSession[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [selectedSubjectId, setSelectedSubjectId] = useState<number | null>(null);
  const [stats, setStats] = useState<FocusStatsSummary | null>(null);
  const [latestAppCheck, setLatestAppCheck] = useState<FocusAppCheck | null>(null);
  const [checklistData, setChecklistData] = useState<ChecklistPageData | null>(null);
  const [isChecklistDrawerOpen, setIsChecklistDrawerOpen] = useState(false);
  const [scheduleData, setScheduleData] = useState<SchedulePageData | null>(null);
  const [isScheduleDrawerOpen, setIsScheduleDrawerOpen] = useState(false);
  const [scheduleDraft, setScheduleDraft] = useState<ScheduleBlockDraft>(() => emptyScheduleDraft(todayString()));
  const [showTodayComposer, setShowTodayComposer] = useState(false);
  const [todayDraft, setTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [editingTodayId, setEditingTodayId] = useState<number | null>(null);
  const [editingTodayDraft, setEditingTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [checklistSaving, setChecklistSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const studyStateRequestRef = useRef(0);
  const suppressNextReminderRef = useRef(false);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const active = studyState.status === 'active';
  const canPause = active && (studyState.phase === 'focus' || studyState.phase === 'awaiting_break');
  const canTogglePause = canPause || (active && studyState.is_paused);
  const canExitNormally = active && studyState.mode === 'normal';
  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);
  const displayedSubjectId = active ? studyState.subject_id : selectedSubjectId;
  const selectedSubjectName = displayedSubjectId ? subjectNameMap.get(displayedSubjectId) : null;
  const currentSubjectLabel = studyState.subject_id ? subjectNameMap.get(studyState.subject_id) ?? '当前科目' : null;
  const todayTaskCount = checklistData?.today_items.length ?? 0;
  const timerValue = studyState.phase === 'idle' ? formatSeconds(focusMinutes * 60) : studyState.phase === 'awaiting_break' ? formatSeconds(studyState.phase_elapsed_seconds) : formatSeconds(studyState.phase_remaining_seconds);
  const activeClockLabel = studyState.is_paused ? '暂停中' : studyState.phase === 'awaiting_break' ? '等待确认休息' : phaseLabel[studyState.phase];
  const whitelistStatusLabel = studyState.focus_enforcement_active ? '白名单执行中' : active && studyState.phase !== 'break' && !studyState.whitelist_enabled ? '白名单已关闭' : '休息阶段';
  const quietMeta = ['第 ' + studyState.cycle_index + ' 轮', '剩余 ' + formatSeconds(studyState.study_remaining_seconds), nextBreakLabel(studyState), latestAppCheck ? foregroundSummary(latestAppCheck) : '前台监控待命'];

  useEffect(() => { void initializePage(); }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<{ active_state_changed?: boolean; took_over_active_mode?: boolean }>(STUDY_SYNC_STATE_CHANGED_EVENT, (event) => {
      if (cancelled || !event.payload?.active_state_changed) return;
      if (event.payload?.took_over_active_mode) suppressNextReminderRef.current = true;
      void refreshStudyState();
      void refreshDashboard();
      void refreshChecklistData();
      void refreshScheduleData();
    }).then((dispose) => { unlisten = dispose; });
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  useEffect(() => {
    const handleFeishuRefresh = () => {
      void refreshDashboard();
      void refreshChecklistData();
      void refreshScheduleData();
    };
    window.addEventListener(FEISHU_SYNC_REFRESH_EVENT, handleFeishuRefresh);
    return () => window.removeEventListener(FEISHU_SYNC_REFRESH_EVENT, handleFeishuRefresh);
  }, []);

  useEffect(() => {
    void setStudyFullscreen(active && ['focus', 'awaiting_break', 'break'].includes(studyState.phase)).catch(() => undefined);
    return () => { if (active) void setStudyFullscreen(false).catch(() => undefined); };
  }, [active, studyState.phase]);

  useEffect(() => {
    if (!active) return undefined;
    const timerId = window.setInterval(() => { void refreshStudyState(); }, 1000);
    return () => window.clearInterval(timerId);
  }, [active]);

  useEffect(() => {
    if (!active) return;
    const key = reminderKey(studyState);
    if (reminderBaselineKey === null) { reminderBaselineKey = key; return; }
    if (key === reminderBaselineKey) return;
    reminderBaselineKey = key;
    if (suppressNextReminderRef.current) {
      suppressNextReminderRef.current = false;
      return;
    }
    const reminder = buildReminder(studyState);
    if (reminder) void notifyStudyReminder(reminder);
  }, [active, studyState]);

  async function initializePage() {
    try {
      const [settings, subjectsData, stateData] = await Promise.all([getAppSettings(), listSubjects(), getStudyModeState()]);
      setStudyMinutes(settings.default_study_minutes);
      setFocusMinutes(settings.default_focus_minutes);
      setBreakMinutes(settings.break_minutes);
      setLongBreakMinutes(settings.long_break_minutes);
      setLongBreakInterval(settings.long_break_interval);
      setMode(settings.default_focus_mode);
      setSubjects(subjectsData);
      setSelectedSubjectId(null);
      const requestId = beginStudyStateRequest();
      applyStudyStateIfCurrent(stateData, requestId);
      reminderBaselineKey = reminderKey(stateData);
      await Promise.all([refreshDashboard(), refreshChecklistData(), refreshScheduleData()]);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  function beginStudyStateRequest() { studyStateRequestRef.current += 1; return studyStateRequestRef.current; }

  function applyStudyStateIfCurrent(nextState: StudyModeState, requestId: number) {
    if (requestId !== studyStateRequestRef.current) return false;
    setStudyState(nextState);
    if (nextState.status !== 'active') {
      setIsChecklistDrawerOpen(false);
      setIsScheduleDrawerOpen(false);
    }
    return true;
  }

  async function refreshDashboard() {
    try {
      const [historyData, statsData] = await Promise.all([listFocusSessions(), getFocusStatsSummary()]);
      setHistory(historyData);
      setStats(statsData);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function refreshChecklistData() {
    try { setChecklistData(await getChecklistPageData()); } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function refreshScheduleData() {
    try {
      const date = todayString();
      setScheduleData(await getSchedulePageData(date));
      setScheduleDraft((current) => ({ ...current, scheduleDate: date, subjectId: studyState.subject_id ?? current.subjectId }));
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function refreshStudyState() {
    try {
      const requestId = beginStudyStateRequest();
      const nextState = await getStudyModeState();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      if (nextState.status !== 'active') await refreshDashboard();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  function queueConfiguredSync(trigger = 'focus_state_change') {
    void syncConfiguredStateChange(trigger).catch(() => undefined);
  }

  function scheduleSubjectCategory(subjectId: number | null | undefined) {
    if (subjectId === 1) return 'politics';
    if (subjectId === 2) return 'english';
    if (subjectId === 3) return 'math';
    if (subjectId === 4) return 'major';
    return 'general';
  }

  async function handleStart() {
    try {
      setError(null); setMonitorError(null); setLatestAppCheck(null); setNotice(null);
      const requestId = beginStudyStateRequest();
      const nextState = await startStudyMode(studyMinutes * 60, focusMinutes * 60, breakMinutes * 60, longBreakMinutes * 60, longBreakInterval, mode, selectedSubjectId, mode === 'strict' ? true : normalWhitelistEnabled);
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(nextState.focus_enforcement_active ? '学习模式已开始。窗口关闭后会进入托盘，后台继续计时并执行白名单。' : '学习模式已开始。窗口关闭后会进入托盘，后台继续计时，白名单已关闭。');
      reminderBaselineKey = reminderKey(nextState);
      void notifyStudyReminder({ title: '学习模式已开始', body: '第 ' + nextState.cycle_index + ' 轮番茄钟开始，专注 ' + formatDuration(nextState.focus_seconds) + '。' });
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function handleConfirmBreak() {
    try {
      setError(null); setMonitorError(null); setLatestAppCheck(null);
      const requestId = beginStudyStateRequest();
      const nextState = await confirmStudyBreak();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(breakKindLabel(nextState.break_kind) + '已开始。休息结束后会自动进入下一轮番茄钟。');
      reminderBaselineKey = reminderKey(nextState);
      void notifyStudyReminder({ title: breakKindLabel(nextState.break_kind) + '开始', body: '休息 ' + formatDuration(nextState.effective_break_seconds) + '，到点后自动进入下一轮番茄钟。' });
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function handleTogglePause() {
    try {
      setError(null);
      const requestId = beginStudyStateRequest();
      const nextState = studyState.is_paused ? await resumeStudyMode() : await pauseStudyMode();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(nextState.is_paused ? (nextState.focus_enforcement_active ? '计时已暂停，白名单仍在执行。' : '计时已暂停，白名单已关闭。') : '已继续学习计时。');
      reminderBaselineKey = reminderKey(nextState);
      if (!nextState.is_paused) {
        const refreshRequestId = beginStudyStateRequest();
        const refreshedState = await getStudyModeState();
        applyStudyStateIfCurrent(refreshedState, refreshRequestId);
        reminderBaselineKey = reminderKey(refreshedState);
      }
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function handleActiveSubjectChange(value: string) {
    try {
      setError(null);
      const subjectId = value ? Number(value) : null;
      const requestId = beginStudyStateRequest();
      const nextState = await updateStudyModeSubject(subjectId);
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice('本次学习科目已更新。');
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function handleNormalExit() {
    if (!window.confirm('确定结束本次学习吗？当前番茄会按已学习时长结束记录。')) return;
    try {
      setError(null); setMonitorError(null); setLatestAppCheck(null);
      const requestId = beginStudyStateRequest();
      const nextState = await resetStudyMode();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice('已结束本次学习。');
      reminderBaselineKey = reminderKey(nextState);
      await refreshDashboard();
      queueConfiguredSync();
      setIsChecklistDrawerOpen(false);
      setIsScheduleDrawerOpen(false);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  async function handleCheckForeground() {
    const sessionId = studyState.current_session?.id;
    if (!sessionId) return;
    try { setMonitorError(null); setLatestAppCheck(await checkFocusForegroundApp(sessionId)); } catch (reason) { setMonitorError(reason instanceof Error ? reason.message : String(reason)); }
  }

  function beginEditTodayItem(item: TodayPlanItem) {
    setEditingTodayId(item.id);
    setEditingTodayDraft({ title: item.title, note: item.note ?? '', dueDate: item.due_date ?? '', subjectId: item.subject_id });
  }

  async function withChecklistRefresh(work: () => Promise<void>, successMessage?: string, trigger = 'local_data_change') {
    try {
      setChecklistSaving(true);
      await work();
      await refreshChecklistData();
      if (successMessage) setNotice(successMessage);
      queueConfiguredSync(trigger);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); } finally { setChecklistSaving(false); }
  }

  async function handleCreateTodayItem() {
    if (!todayDraft.title.trim()) return;
    await withChecklistRefresh(async () => {
      await createTodayPlanItem({ ...todayDraft, subjectId: todayDraft.subjectId ?? studyState.subject_id ?? null });
      setTodayDraft({ ...emptyTodayDraft, subjectId: studyState.subject_id ?? null });
      setShowTodayComposer(false);
    }, '今日任务已添加。');
  }

  async function handleSaveTodayEdit() {
    if (editingTodayId === null || !editingTodayDraft.title.trim()) return;
    await withChecklistRefresh(async () => {
      await updateTodayPlanItem(editingTodayId, editingTodayDraft);
      setEditingTodayId(null);
      setEditingTodayDraft(emptyTodayDraft);
    }, '今日任务已更新。');
  }

  async function handleDeleteTodayItem(itemId: number) {
    await withChecklistRefresh(async () => { await deleteTodayPlanItem(itemId); }, '今日任务已删除。');
  }

  async function handleCompleteTodayItem(item: TodayPlanItem) {
    const nextCompleted = !item.completed;
    let syncSourceCompletion = false;
    if (nextCompleted && item.source_task_id !== null) syncSourceCompletion = window.confirm('完成今日任务时，也同步完成源待办吗？');
    await withChecklistRefresh(async () => { await completeTodayPlanItem(item.id, nextCompleted, syncSourceCompletion); }, nextCompleted ? '今日任务已完成。' : '今日任务已恢复为未完成。');
  }

  async function handleTodayDrawerDragEnd(event: DragEndEvent) {
    if (!checklistData || !event.over) return;
    const activeId = String(event.active.id);
    const overId = String(event.over.id);
    if (!activeId.startsWith('today:')) return;
    const items = [...checklistData.today_items];
    const fromId = Number(activeId.slice(6));
    const fromIndex = items.findIndex((item) => item.id === fromId);
    if (fromIndex === -1) return;
    let targetIndex = fromIndex;
    if (overId === FOCUS_TODAY_CONTAINER_ID) targetIndex = items.length - 1;
    else if (overId.startsWith('today:')) targetIndex = items.findIndex((item) => item.id === Number(overId.slice(6)));
    else return;
    if (targetIndex === -1 || targetIndex === fromIndex) return;
    const reordered = arrayMove(items, fromIndex, targetIndex);
    setChecklistData({ ...checklistData, today_items: reordered });
    await withChecklistRefresh(async () => { await reorderTodayPlanItems(reordered.map((item) => item.id)); });
  }

  function handleToggleChecklistDrawer() {
    const willOpen = !isChecklistDrawerOpen;
    setIsChecklistDrawerOpen(willOpen);
    if (willOpen) setIsScheduleDrawerOpen(false);
    if (willOpen) void refreshChecklistData();
  }

  function handleToggleScheduleDrawer() {
    const willOpen = !isScheduleDrawerOpen;
    setIsScheduleDrawerOpen(willOpen);
    if (willOpen) {
      setIsChecklistDrawerOpen(false);
      void refreshScheduleData();
    }
  }

  async function handleCreateScheduleBlock() {
    if (!scheduleDraft.title.trim()) return;
    const subjectId = studyState.subject_id ?? scheduleDraft.subjectId ?? null;
    await withChecklistRefresh(async () => {
      await createScheduleBlock({
        ...scheduleDraft,
        subjectId,
        categoryKey: scheduleSubjectCategory(subjectId),
      });
      setScheduleDraft(emptyScheduleDraft(todayString()));
      await refreshScheduleData();
    }, '课表时间块已添加。');
  }

  async function handleDeleteScheduleBlock(blockId: number) {
    await withChecklistRefresh(async () => {
      await deleteScheduleBlock(blockId);
      await refreshScheduleData();
    }, '课表时间块已删除。');
  }

  async function handleStartScheduleBlock(block: ScheduleBlock) {
    try {
      const settings = await getAppSettings();
      const requestId = beginStudyStateRequest();
      const nextState = await startStudyModeFromScheduleBlock(
        block.id,
        settings.default_study_minutes * 60,
        settings.default_focus_minutes * 60,
        settings.break_minutes * 60,
        settings.long_break_minutes * 60,
        settings.long_break_interval,
        settings.default_focus_mode,
      );
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice('已从课表开始专注。');
      setIsScheduleDrawerOpen(false);
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  if (active) {
    return (
      <DndContext collisionDetection={closestCenter} onDragEnd={(event) => void handleTodayDrawerDragEnd(event)} sensors={sensors}>
        <section className={'focus-active-shell phase-' + studyState.phase + (studyState.is_paused ? ' is-paused' : '') + ((isChecklistDrawerOpen || isScheduleDrawerOpen) ? ' is-drawer-open' : '')}>
          <div className="focus-active-bg" aria-hidden="true" />
          <header className="focus-active-header">
            <span className="focus-minimal-status">{studyState.is_paused ? '计时暂停' : phaseLabel[studyState.phase]}</span>
            <label className="subject-switch fullscreen-subject-switch top-subject-switch">
              <span>科目</span>
              <select aria-label="当前科目" className="select-input" disabled={subjects.length === 0} onChange={(event) => void handleActiveSubjectChange(event.target.value)} value={studyState.subject_id ?? ''}>
                <option value="">未指定</option>
                {subjects.map((subject) => <option disabled={!subject.enabled} key={subject.id} value={subject.id}>{subject.name}</option>)}
              </select>
            </label>
            <div className="focus-header-right">
              <button aria-expanded={isChecklistDrawerOpen} aria-label="打开今日任务" className={'focus-hud-card focus-hud-task' + (isChecklistDrawerOpen ? ' is-active' : '')} onClick={handleToggleChecklistDrawer} title="今日任务" type="button">
                <span className="focus-hud-icon"><ClipboardList size={15} /></span>
                <span className="focus-hud-copy">
                  <span>今日任务</span>
                  <strong>{todayTaskCount} 项</strong>
                </span>
              </button>
              <button aria-expanded={isScheduleDrawerOpen} aria-label="打开今日课表" className={'focus-hud-card focus-hud-task' + (isScheduleDrawerOpen ? ' is-active' : '')} onClick={handleToggleScheduleDrawer} title="今日课表" type="button">
                <span className="focus-hud-icon"><CalendarClock size={15} /></span>
                <span className="focus-hud-copy">
                  <span>今日课表</span>
                  <strong>{scheduleData?.day_blocks.length ?? 0} 块</strong>
                </span>
              </button>
              <div className="live-badge"><span className={studyState.focus_enforcement_active ? 'live-dot on' : 'live-dot'} />{whitelistStatusLabel}</div>
            </div>
          </header>

          {(error || notice || monitorError) && <div className="focus-notice-stack">{error && <p className="alert error">{error}</p>}{notice && <p className="alert success">{notice}</p>}{monitorError && <p className="alert error">前台检测失败：{monitorError}</p>}</div>}

          <main className="focus-clock-zone">
            <p>{activeClockLabel}</p>
            <strong>{timerValue}</strong>
            <span>{buildPhaseMessage(studyState)}</span>
            <div className="focus-round-controls">
              {canTogglePause && <button aria-label={studyState.is_paused ? '继续计时' : '暂停计时'} className={studyState.is_paused ? 'focus-round-button primary' : 'focus-round-button'} onClick={handleTogglePause} title={studyState.is_paused ? '继续计时' : '暂停'} type="button">{studyState.is_paused ? <Play size={28} /> : <Pause size={28} />}</button>}
              {studyState.phase === 'awaiting_break' && <button aria-label={'确认开始' + breakKindLabel(studyState.break_kind)} className="focus-round-button secondary" disabled={studyState.is_paused} onClick={handleConfirmBreak} title={'确认开始' + breakKindLabel(studyState.break_kind)} type="button"><Coffee size={26} /></button>}
            </div>
          </main>

          <footer className="focus-active-footer">
            <div className="focus-quiet-meta">{quietMeta.map((item) => <span key={item}>{item}</span>)}</div>
            <div className="focus-quiet-actions">
              <button aria-label="刷新前台状态" className="focus-hud-card focus-command-button" onClick={() => void handleCheckForeground()} title="刷新前台状态" type="button">
                <span className="focus-hud-icon"><Gauge size={14} /></span>
                <span className="focus-hud-copy">
                  <span>刷新状态</span>
                  <strong>前台检测</strong>
                </span>
              </button>
              {canExitNormally && (
                <button aria-label="结束学习" className="focus-hud-card focus-command-button" onClick={() => void handleNormalExit()} title="结束学习" type="button">
                  <span className="focus-hud-icon"><Square size={14} /></span>
                  <span className="focus-hud-copy">
                    <span>结束学习</span>
                    <strong>普通模式</strong>
                  </span>
                </button>
              )}
            </div>
          </footer>

          <TodayPlanDrawer compact dndContainerId={FOCUS_TODAY_CONTAINER_ID} dndIsOver={false} currentSubjectLabel={currentSubjectLabel} editingTodayDraft={editingTodayDraft} editingTodayId={editingTodayId} emptyDescription="可以在清单页拖入，也可以直接在这里补一条今天要推进的任务。" emptyTitle="今天还没有进入任务" isOpen={isChecklistDrawerOpen} items={checklistData?.today_items ?? []} onBeginEdit={beginEditTodayItem} onCancelEdit={() => setEditingTodayId(null)} onChangeEdit={(patch) => setEditingTodayDraft((current) => ({ ...(current ?? emptyTodayDraft), ...patch }))} onClose={() => setIsChecklistDrawerOpen(false)} onComplete={(item) => void handleCompleteTodayItem(item)} onCreate={() => void handleCreateTodayItem()} onDelete={(itemId) => void handleDeleteTodayItem(itemId)} onDraftChange={(patch) => setTodayDraft((current) => ({ ...current, ...patch }))} onRefresh={() => void refreshChecklistData()} onSaveEdit={() => void handleSaveTodayEdit()} onToggleComposer={() => setShowTodayComposer((current) => !current)} saving={checklistSaving} showComposer={showTodayComposer} sortable subtitle="Today Queue" title="今日任务" getItemDragId={(item) => getTodaySortableId(item.id)} todayDate={checklistData?.today_date ?? ''} todayDraft={todayDraft} variant="drawer" />
          <ScheduleDrawer canStart={false} isOpen={isScheduleDrawerOpen} data={scheduleData} draft={scheduleDraft} saving={checklistSaving} onClose={() => setIsScheduleDrawerOpen(false)} onRefresh={() => void refreshScheduleData()} onDraftChange={(patch) => setScheduleDraft((current) => ({ ...current, ...patch }))} onCreate={() => void handleCreateScheduleBlock()} onDelete={(blockId) => void handleDeleteScheduleBlock(blockId)} onStart={(block) => void handleStartScheduleBlock(block)} />
        </section>
      </DndContext>
    );
  }

  return (
    <section className="page-shell focus-prepare-shell">
      <header className="page-header prepare-header">
        <div><p className="eyebrow">Focus Ritual</p><h2>进入学习模式</h2><p>设定本次学习长度、番茄节奏和休息规则。开始后界面会切换为极简番茄钟，配置入口自动锁定。</p></div>
        <div className={'phase-badge phase-' + studyState.phase}><span>{phaseLabel[studyState.phase]}</span><strong>{studyState.phase === 'finished' ? '已收束' : '待命'}</strong></div>
      </header>
      {error && <p className="alert error">{error}</p>}{notice && <p className="alert success">{notice}</p>}{monitorError && <p className="alert error">前台检测失败：{monitorError}</p>}
      <div className="prepare-grid">
        <section className="start-console">
          <div className="timer-orbit"><span>下一轮专注</span><strong>{formatSeconds(focusMinutes * 60)}</strong><p>{selectedSubjectName ?? '未指定科目'} / {mode === 'strict' ? '强制模式' : '普通模式'}</p></div>
          <div className="console-facts"><CoreFact label="学习模式" value={formatDuration(studyMinutes * 60)} /><CoreFact label="番茄时长" value={formatDuration(focusMinutes * 60)} /><CoreFact label="短休息" value={formatDuration(breakMinutes * 60)} /><CoreFact label="长休息" value={formatDuration(longBreakMinutes * 60) + ' / ' + longBreakInterval + ' 轮'} /></div>
          <button className="start-ritual-button" onClick={handleStart} type="button"><Play size={22} />开始学习</button>
        </section>
        <aside className="control-panel">
          <div className="panel-title"><div><p className="eyebrow">Plan</p><h3>本次节奏</h3></div><BookOpen size={20} /></div>
          <div className="number-grid"><NumberField label="学习模式" onChange={setStudyMinutes} value={studyMinutes} /><NumberField label="番茄钟" onChange={setFocusMinutes} value={focusMinutes} /><NumberField label="短休息" onChange={setBreakMinutes} value={breakMinutes} /><NumberField label="长休息" onChange={setLongBreakMinutes} value={longBreakMinutes} /></div>
          <PresetStrip items={studyPresetMinutes} prefix="学习 " selected={studyMinutes} suffix="m" onSelect={setStudyMinutes} /><PresetStrip items={focusPresetMinutes} prefix="番茄 " selected={focusMinutes} suffix="m" onSelect={setFocusMinutes} /><PresetStrip items={breakPresetMinutes} prefix="短休 " selected={breakMinutes} suffix="m" onSelect={setBreakMinutes} /><PresetStrip items={longBreakPresetMinutes} prefix="长休 " selected={longBreakMinutes} suffix="m" onSelect={setLongBreakMinutes} /><PresetStrip items={longBreakIntervalPresets} prefix="每 " selected={longBreakInterval} suffix=" 轮长休" onSelect={setLongBreakInterval} />
          <label className="field-block"><span>科目</span><select className="select-input" disabled={subjects.length === 0} onChange={(event) => setSelectedSubjectId(event.target.value ? Number(event.target.value) : null)} value={selectedSubjectId ?? ''}><option value="">不指定</option>{subjects.map((subject) => <option disabled={!subject.enabled} key={subject.id} value={subject.id}>{subject.name}</option>)}</select></label>
          <div className="segmented-control"><button className={mode === 'normal' ? 'active' : ''} onClick={() => setMode('normal')} type="button">普通模式</button><button className={mode === 'strict' ? 'active' : ''} onClick={() => setMode('strict')} type="button">强制模式</button></div>
          {mode === 'normal' && (
            <label className="capability-row focus-whitelist-toggle">
              <span>启用白名单</span>
              <input checked={normalWhitelistEnabled} onChange={(event) => setNormalWhitelistEnabled(event.target.checked)} role="switch" type="checkbox" />
            </label>
          )}
        </aside>
      </div>
      <div className="dashboard-strip">
        <section className="soft-panel"><div className="panel-title"><div><p className="eyebrow">Today</p><h3>今日状态</h3></div><CheckCircle2 size={20} /></div><div className="stats-grid four"><Metric icon={Timer} label="今日" value={formatDuration(stats?.today_seconds ?? 0)} /><Metric icon={Timer} label="本周" value={formatDuration(stats?.week_seconds ?? 0)} /><Metric icon={Timer} label="本月" value={formatDuration(stats?.month_seconds ?? 0)} /><Metric icon={ShieldCheck} label="拦截" value={String(stats?.interruption_count ?? 0)} /></div></section>
        <section className="soft-panel"><div className="panel-title"><div><p className="eyebrow">Monitor</p><h3>白名单状态</h3></div><Gauge size={20} /></div><div className="monitor-callout"><Leaf size={18} /><div><strong>准备就绪</strong><p>{mode === 'normal' && !normalWhitelistEnabled ? '普通模式本次不执行白名单。' : '开始学习后会持续检查前台窗口，并关闭非白名单软件或网站。'}</p></div></div></section>
        <section className="soft-panel history-panel"><div className="panel-title"><div><p className="eyebrow">History</p><h3>最近记录</h3></div><BellRing size={20} /></div>{history.length === 0 ? <div className="empty-state compact">还没有专注记录。</div> : <div className="compact-history">{history.slice(0, 4).map((session) => <article className="list-row compact-row" key={session.id}><div><strong>{session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未知科目' : '未指定科目'}</strong><p>{formatSessionTimeRange(session)}</p></div><div className="history-meta"><span>{sessionStatusLabel(session.status)}</span><strong>{formatDuration(session.actual_seconds || session.planned_seconds)}</strong></div></article>)}</div>}</section>
      </div>
    </section>
  );
}

function breakKindLabel(kind: StudyModeState['break_kind']) { return kind === 'long' ? '长休息' : '短休息'; }
function nextBreakLabel(studyState: StudyModeState) { return breakKindLabel(studyState.break_kind) + ' ' + formatDuration(studyState.effective_break_seconds || studyState.break_seconds); }
function buildPhaseMessage(studyState: StudyModeState) {
  if (studyState.is_paused) return studyState.focus_enforcement_active ? '计时暂停，白名单仍在强制执行' : '计时暂停，白名单已关闭';
  if (studyState.phase === 'focus') return '第 ' + studyState.cycle_index + ' 轮番茄钟进行中';
  if (studyState.phase === 'awaiting_break') return '本轮已到点，确认后进入 ' + nextBreakLabel(studyState);
  if (studyState.phase === 'break') return breakKindLabel(studyState.break_kind) + '进行中';
  if (studyState.phase === 'finished') return '学习模式已完成';
  if (studyState.phase === 'emergency_exited') return '历史退出状态';
  return '设置节奏后开始';
}
function reminderKey(studyState: StudyModeState) { return [studyState.id ?? 'idle', studyState.status, studyState.phase, studyState.cycle_index, studyState.break_kind].join(':'); }
function buildReminder(studyState: StudyModeState) {
  if (studyState.status === 'active' && studyState.phase === 'focus') return { title: studyState.cycle_index > 1 ? '下一轮番茄钟开始' : '番茄钟开始', body: '第 ' + studyState.cycle_index + ' 轮开始，专注 ' + formatDuration(studyState.focus_seconds) + '。' };
  if (studyState.status === 'active' && studyState.phase === 'awaiting_break') return { title: '番茄钟结束', body: '本轮已经到点。确认后进入 ' + nextBreakLabel(studyState) + '；未确认前学习时间继续累计。' };
  if (studyState.status === 'active' && studyState.phase === 'break') return { title: breakKindLabel(studyState.break_kind) + '开始', body: formatDuration(studyState.effective_break_seconds) + ' 后自动进入下一轮番茄钟。' };
  if (studyState.status === 'finished' || studyState.phase === 'finished') return { title: '学习模式完成', body: '本次学习已完成，共累计 ' + formatDuration(studyState.study_elapsed_seconds) + '。' };
  return null;
}
function foregroundSummary(check: FocusAppCheck) { return (check.match_result.allowed ? '已放行' : '已拦截') + ' / ' + check.foreground_app.process_name + ' / ' + (check.foreground_app.window_title || '无窗口标题'); }
function getTodaySortableId(itemId: number) { return 'today:' + itemId; }
function Metric({ icon: Icon, label, value }: { icon: typeof Timer; label: string; value: string }) { return <article className="metric-card"><Icon size={18} /><span>{label}</span><strong>{value}</strong></article>; }
function CoreFact({ label, value }: { label: string; value: string }) { return <article className="core-fact"><span>{label}</span><strong>{value}</strong></article>; }
function NumberField({ label, onChange, value }: { label: string; onChange: (value: number) => void; value: number }) { return <label className="field-block"><span>{label}</span><input className="number-input" min={1} onChange={(event) => onChange(Number(event.target.value) || 1)} type="number" value={value} /></label>; }
function PresetStrip({ items, onSelect, prefix, selected, suffix }: { items: number[]; onSelect: (value: number) => void; prefix: string; selected: number; suffix: string }) { return <div className="preset-strip">{items.map((value) => <button className={selected === value ? 'chip active' : 'chip'} key={prefix + '-' + value} onClick={() => onSelect(value)} type="button">{prefix}{value}{suffix}</button>)}</div>; }

import { useEffect, useMemo, useRef, useState } from 'react';
import { closestCenter, DndContext, PointerSensor, useSensor, useSensors, type DragEndEvent } from '@dnd-kit/core';
import { arrayMove } from '@dnd-kit/sortable';
import { BellRing, BookOpen, CalendarClock, CheckCircle2, ClipboardList, Coffee, Gauge, Leaf, Pause, Play, ShieldCheck, Square, Timer } from 'lucide-react';
import ConfirmDialog from '../components/ConfirmDialog';
import LearningHub from '../components/focus/LearningHub';
import ScheduleDrawer from '../components/ScheduleDrawer';
import TodayPlanDrawer from '../components/TodayPlanDrawer';
import { completeTodayPlanItem, createTodayPlanItem, deleteTodayPlanItem, getChecklistPageData, reorderTodayPlanItems, updateTodayPlanItem } from '../services/checklistApi';
import { confirmStudyBreak, getFocusStatsSummary, getStudyModeState, listFocusSessions, listSubjects, pauseStudyMode, resetStudyMode, resumeStudyMode, startStudyMode, updateStudyModeSubject } from '../services/focusApi';
import { notifyStudyReminder } from '../services/alertApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { createScheduleBlock, createScheduleBlockFromTodayItem, deleteScheduleBlock, getSchedulePageData, startStudyModeFromScheduleBlock } from '../services/scheduleApi';
import { getAppSettings, getSyncDeviceId, saveAppSettings } from '../services/settingsApi';
import { buildStudyReminder, isFinishedStudyMode, isStaleFinishedStudyReminder, markStudyReminderSeen, nextStudyBreakLabel, registerStudyReminderScope, resetStudyReminderScope, studyBreakKindLabel } from '../services/studyReminder';
import { STUDY_SYNC_STATE_CHANGED_EVENT, syncConfiguredStateChange } from '../services/syncApi';
import { FEISHU_SYNC_REFRESH_EVENT } from '../services/feishuApi';
import { setStudyFullscreen } from '../services/systemApi';
import { listenTauriEvent } from '../services/tauriEvents';
import { isTauriRuntime } from '../services/tauriInvoke';
import type { ChecklistPageData, TodayPlanItem, TodayPlanItemDraft } from '../types/checklist';
import type { FocusMode, FocusSession, FocusStatsSummary, StudyModePhase, StudyModeState, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';
import type { ScheduleBlock, ScheduleBlockDraft, SchedulePageData } from '../types/schedule';
import type { AppSettings } from '../types/settings';
import { currentMinuteOfDay, formatDateKey } from '../utils/date';
import { recommendScheduleBlock, type ScheduleRecommendation } from '../utils/scheduleRecommendation';

const studyPresetMinutes = [60, 120, 180, 240];
const focusPresetMinutes = [25, 45, 60, 90];
const breakPresetMinutes = [5, 10, 15, 20];
const longBreakPresetMinutes = [10, 15, 20, 30];
const longBreakIntervalPresets = [2, 3, 4, 6];
const ACTIVE_STATE_CALIBRATION_INTERVAL_MS = 15 * 1000;
const FOCUS_TODAY_CONTAINER_ID = 'focus-today-container';
const QUICK_SCHEDULE_DAY_START = 6 * 60;
const QUICK_SCHEDULE_DAY_END = 24 * 60;
const QUICK_SCHEDULE_SLOT_MINUTES = 15;
const emptyTodayDraft: TodayPlanItemDraft = { title: '', note: '', dueDate: '', subjectId: null };
type FocusConfirmRequest = { kind: 'normalExit' } | { kind: 'syncSourceCompletion'; item: TodayPlanItem };
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

function formatMinuteOfDay(minute: number) {
  const safeMinute = Math.max(0, Math.min(24 * 60, Math.floor(minute)));
  const hours = Math.floor(safeMinute / 60).toString().padStart(2, '0');
  const minutes = (safeMinute % 60).toString().padStart(2, '0');
  return `${hours}:${minutes}`;
}

function roundUpToScheduleSlot(minute: number) {
  return Math.ceil(minute / QUICK_SCHEDULE_SLOT_MINUTES) * QUICK_SCHEDULE_SLOT_MINUTES;
}

function findNextAvailableScheduleSlot(blocks: ScheduleBlock[], fromMinute: number, durationMinutes: number) {
  const duration = Math.max(QUICK_SCHEDULE_SLOT_MINUTES, Math.ceil(durationMinutes / QUICK_SCHEDULE_SLOT_MINUTES) * QUICK_SCHEDULE_SLOT_MINUTES);
  let startMinute = roundUpToScheduleSlot(Math.max(QUICK_SCHEDULE_DAY_START, fromMinute));
  const sortedBlocks = [...blocks].sort((left, right) => left.start_minute - right.start_minute || left.end_minute - right.end_minute);

  while (startMinute + duration <= QUICK_SCHEDULE_DAY_END) {
    const overlap = sortedBlocks.find((block) => startMinute < block.end_minute && block.start_minute < startMinute + duration);
    if (!overlap) {
      return { startMinute, endMinute: startMinute + duration };
    }
    startMinute = roundUpToScheduleSlot(Math.max(startMinute + QUICK_SCHEDULE_SLOT_MINUTES, overlap.end_minute));
  }

  return null;
}

function describeScheduleRecommendation(
  recommendation: ScheduleRecommendation | null,
  subjectNameMap: Map<number, string>,
) {
  if (!recommendation) {
    return null;
  }

  const kindLabel =
    recommendation.kind === 'current' ? '当前' : recommendation.kind === 'next' ? '下一个' : '遗漏';
  const block = recommendation.block;
  const subjectLabel = block.subject_id ? subjectNameMap.get(block.subject_id) ?? '未指定科目' : '未指定科目';

  return `${kindLabel}课表块 ${formatMinuteOfDay(block.start_minute)}-${formatMinuteOfDay(block.end_minute)} · ${block.title} / ${subjectLabel}`;
}

function secondsSince(value: string | null, now: number) {
  if (!value) return 0;
  const timestamp = new Date(value).getTime();
  if (!Number.isFinite(timestamp)) return 0;
  return Math.max(0, Math.floor((now - timestamp) / 1000));
}

function localPhaseSeconds(studyState: StudyModeState, now: number) {
  if (studyState.is_paused || studyState.status !== 'active') {
    return studyState.phase === 'awaiting_break'
      ? studyState.phase_elapsed_seconds
      : studyState.phase_remaining_seconds;
  }

  if (studyState.phase === 'awaiting_break') {
    return Math.max(studyState.phase_elapsed_seconds, secondsSince(studyState.phase_started_at, now));
  }

  const phaseDuration = studyState.phase === 'break'
    ? studyState.effective_break_seconds || studyState.break_seconds
    : studyState.focus_seconds;
  const elapsed = secondsSince(studyState.phase_started_at, now);
  return Math.max(0, phaseDuration - elapsed);
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

function sessionStatusLabel(status: string) {
  const labels: Record<string, string> = { running: '进行中', finished: '已完成', interrupted: '已中断', emergency_exited: '已退出' };
  return labels[status] ?? status;
}

function shouldSilentlyMarkInitialReminderSeen(studyState: StudyModeState) {
  if (studyState.status === 'active' && studyState.phase === 'awaiting_break') {
    return false;
  }
  if (isFinishedStudyMode(studyState)) {
    return isStaleFinishedStudyReminder(studyState);
  }
  return true;
}

export default function FocusPage() {
  const [studyMinutes, setStudyMinutes] = useState(120);
  const [focusMinutes, setFocusMinutes] = useState(25);
  const [breakMinutes, setBreakMinutes] = useState(5);
  const [longBreakMinutes, setLongBreakMinutes] = useState(15);
  const [longBreakInterval, setLongBreakInterval] = useState(4);
  const [mode, setMode] = useState<FocusMode>('normal');
  const [whitelistMode, setWhitelistMode] = useState<AppSettings['whitelist_mode']>('allowlist');
  const [autoStartBreakAfterFocus, setAutoStartBreakAfterFocus] = useState(false);
  const [normalWhitelistEnabled, setNormalWhitelistEnabled] = useState(true);
  const [syncDeviceId, setSyncDeviceId] = useState<string | null>(null);
  const [primaryOwnerDeviceId, setPrimaryOwnerDeviceId] = useState<string | null>(null);
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
  const [scheduleDraft, setScheduleDraft] = useState<ScheduleBlockDraft>(() => emptyScheduleDraft(formatDateKey()));
  const [showTodayComposer, setShowTodayComposer] = useState(false);
  const [todayDraft, setTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [editingTodayId, setEditingTodayId] = useState<number | null>(null);
  const [editingTodayDraft, setEditingTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [checklistSaving, setChecklistSaving] = useState(false);
  const [isStartingStudy, setIsStartingStudy] = useState(false);
  const [isQuickSchedulingTask, setIsQuickSchedulingTask] = useState(false);
  const [localClockNow, setLocalClockNow] = useState(() => Date.now());
  const [pendingConfirm, setPendingConfirm] = useState<FocusConfirmRequest | null>(null);
  const [confirmLoading, setConfirmLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const studyStateRequestRef = useRef(0);
  const suppressNextReminderRef = useRef(false);
  const activeReminderScopeRef = useRef<string | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const active = studyState.status === 'active';
  const canPause = active && (studyState.phase === 'focus' || studyState.phase === 'awaiting_break');
  const canTogglePause = canPause || (active && studyState.is_paused);
  const canExitNormally = active && studyState.mode === 'normal';
  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);
  const displayedSubjectId = active ? studyState.subject_id : selectedSubjectId;
  const selectedSubjectName = displayedSubjectId ? subjectNameMap.get(displayedSubjectId) : null;
  const currentSubjectLabel = studyState.subject_id ? subjectNameMap.get(studyState.subject_id) ?? '当前科目' : null;
  const todayItems = checklistData?.today_items ?? [];
  const todayTaskCount = todayItems.length;
  const pendingTodayCount = todayItems.filter((item) => !item.completed).length;
  const completedTodayItems = todayTaskCount - pendingTodayCount;
  const nextTodayTask = todayItems.find((item) => !item.completed) ?? null;
  const scheduleRecommendation = useMemo(
    () => (scheduleData ? recommendScheduleBlock(scheduleData.day_blocks, currentMinuteOfDay(new Date(localClockNow))) : null),
    [localClockNow, scheduleData],
  );
  const scheduleRecommendationBlock = scheduleRecommendation?.block ?? null;
  const scheduleRecommendationMeta = describeScheduleRecommendation(scheduleRecommendation, subjectNameMap);
  const desktopReady = isTauriRuntime();
  const quickScheduleDisabled = checklistSaving || isQuickSchedulingTask || !nextTodayTask;
  const timerValue = studyState.phase === 'idle' ? formatSeconds(focusMinutes * 60) : formatSeconds(localPhaseSeconds(studyState, localClockNow));
  const activeClockLabel = studyState.is_paused ? '暂停中' : studyState.phase === 'awaiting_break' ? '等待确认休息' : phaseLabel[studyState.phase];
  const ruleModeLabel = whitelistMode === 'blocklist' ? '黑名单' : '白名单';
  const ruleActionLabel = whitelistMode === 'blocklist' ? '拦截命中规则' : '拦截未命中规则';
  const whitelistStatusLabel = studyState.focus_enforcement_active ? `${ruleModeLabel}执行中` : active && studyState.phase !== 'break' && !studyState.whitelist_enabled ? '前台规则已关闭' : '休息阶段';
  const activeModeLabel = studyState.mode === 'strict' ? '强制模式' : '普通模式';
  const activeModeMessage = buildActiveModeMessage(studyState, ruleModeLabel);
  const isPrimaryDevice = Boolean(syncDeviceId && primaryOwnerDeviceId === syncDeviceId);
  const primaryStatusLabel = isPrimaryDevice ? '当前为主端' : primaryOwnerDeviceId ? '当前非主端' : '未设置主端';
  const quietMeta = [activeModeLabel, '第 ' + studyState.cycle_index + ' 轮', '剩余 ' + formatSeconds(studyState.study_remaining_seconds), nextBreakLabel(studyState), primaryStatusLabel, latestAppCheck ? foregroundSummary(latestAppCheck) : '前台监控待命'];

  useEffect(() => { void initializePage(); }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenTauriEvent<{ active_state_changed?: boolean; took_over_active_mode?: boolean; primary_owner_changed?: boolean }>(STUDY_SYNC_STATE_CHANGED_EVENT, (event) => {
      if (cancelled) return;
      void refreshPrimaryOwner();
      if (!event.payload?.active_state_changed) return;
      if (event.payload?.took_over_active_mode) suppressNextReminderRef.current = true;
      void refreshStudyState();
      void refreshDashboard();
      void refreshChecklistData();
      void refreshScheduleData();
    }).then((dispose) => { unlisten = dispose; }).catch(() => {
      // Browser previews and partial desktop runtimes should stay in degraded mode quietly.
    });
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
    const timerId = window.setInterval(() => { setLocalClockNow(Date.now()); }, 1000);
    return () => window.clearInterval(timerId);
  }, [active]);

  useEffect(() => {
    if (!active) return undefined;
    const timerId = window.setInterval(() => { void refreshStudyState(); }, ACTIVE_STATE_CALIBRATION_INTERVAL_MS);
    return () => window.clearInterval(timerId);
  }, [active]);

  useEffect(() => {
    const completed = studyState.status === 'finished' || studyState.phase === 'finished';
    if (!active && !completed) {
      activeReminderScopeRef.current = null;
      return;
    }
    registerStudyReminderScope(studyState, activeReminderScopeRef);
    if (suppressNextReminderRef.current) {
      suppressNextReminderRef.current = false;
      markStudyReminderSeen(studyState, syncDeviceId);
      return;
    }
    const reminder = buildStudyReminder(studyState);
    if (!reminder) return;
    if (!markStudyReminderSeen(studyState, syncDeviceId)) return;
    void notifyStudyReminder(reminder);
  }, [active, studyState, syncDeviceId]);

  async function initializePage() {
    try {
      const [settings, subjectsData, stateData, deviceId] = await Promise.all([getAppSettings(), listSubjects(), getStudyModeState(), getSyncDeviceId()]);
      setStudyMinutes(settings.default_study_minutes);
      setFocusMinutes(settings.default_focus_minutes);
      setBreakMinutes(settings.break_minutes);
      setLongBreakMinutes(settings.long_break_minutes);
      setLongBreakInterval(settings.long_break_interval);
      setMode(settings.default_focus_mode);
      setWhitelistMode(settings.whitelist_mode);
      setAutoStartBreakAfterFocus(settings.auto_start_break_after_focus);
      setPrimaryOwnerDeviceId(settings.primary_owner_device_id);
      setSyncDeviceId(deviceId);
      setSubjects(subjectsData);
      setSelectedSubjectId(null);
      const requestId = beginStudyStateRequest();
      applyStudyStateIfCurrent(stateData, requestId);
      registerStudyReminderScope(stateData, activeReminderScopeRef);
      if (shouldSilentlyMarkInitialReminderSeen(stateData)) {
        markStudyReminderSeen(stateData, deviceId);
      }
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
      const date = formatDateKey();
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

  async function refreshPrimaryOwner() {
    try {
      const settings = await getAppSettings();
      setPrimaryOwnerDeviceId(settings.primary_owner_device_id);
    } catch {
    }
  }

  function queueConfiguredSync(trigger = 'focus_state_change') {
    void syncConfiguredStateChange(trigger).catch(() => undefined);
  }

  async function handlePrimaryOwnerChange(checked: boolean) {
    if (!syncDeviceId) return;
    try {
      setError(null);
      const settings = await getAppSettings();
      const nextOwner = checked ? syncDeviceId : settings.primary_owner_device_id === syncDeviceId ? null : settings.primary_owner_device_id;
      const nextOwnerUpdatedAt = Math.max(Date.now(), (settings.primary_owner_updated_at ?? 0) + 1);
      const saved = await saveAppSettings({ ...settings, primary_owner_device_id: nextOwner, primary_owner_updated_at: nextOwnerUpdatedAt });
      setPrimaryOwnerDeviceId(saved.primary_owner_device_id);
      if (saved.sync_backend === 'object_storage') queueConfiguredSync('primary_owner_change');
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  function scheduleSubjectCategory(subjectId: number | null | undefined) {
    if (subjectId === 1) return 'politics';
    if (subjectId === 2) return 'english';
    if (subjectId === 3) return 'math';
    if (subjectId === 4) return 'major';
    return 'general';
  }

  async function handleStart() {
    if (isStartingStudy) return;
    try {
      setIsStartingStudy(true);
      setError(null); setMonitorError(null); setLatestAppCheck(null); setNotice(null);
      const requestId = beginStudyStateRequest();
      const nextState = await startStudyMode(studyMinutes * 60, focusMinutes * 60, breakMinutes * 60, longBreakMinutes * 60, longBreakInterval, mode, selectedSubjectId, mode === 'strict' ? true : normalWhitelistEnabled);
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(nextState.focus_enforcement_active ? `学习模式已开始。窗口关闭后会进入托盘，后台继续计时并执行${ruleModeLabel}。` : '学习模式已开始。窗口关闭后会进入托盘，后台继续计时，前台规则已关闭。');
      resetStudyReminderScope(nextState, activeReminderScopeRef);
      markStudyReminderSeen(nextState, syncDeviceId);
      void notifyStudyReminder({ title: '学习模式已开始', body: '第 ' + nextState.cycle_index + ' 轮番茄钟开始，专注 ' + formatDuration(nextState.focus_seconds) + '。' });
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setIsStartingStudy(false); }
  }

  async function handleConfirmBreak() {
    try {
      setError(null); setMonitorError(null); setLatestAppCheck(null);
      const requestId = beginStudyStateRequest();
      const nextState = await confirmStudyBreak();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(breakKindLabel(nextState.break_kind) + '已开始。休息结束后会自动进入下一轮番茄钟。');
      markStudyReminderSeen(nextState, syncDeviceId);
      void notifyStudyReminder({ title: breakKindLabel(nextState.break_kind) + '开始', body: '休息 ' + formatDuration(nextState.effective_break_seconds) + '，到点后自动进入下一轮番茄钟。' });
      await refreshDashboard();
      queueConfiguredSync();
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  useEffect(() => {
    if (!autoStartBreakAfterFocus || !active || studyState.phase !== 'awaiting_break' || studyState.is_paused) {
      return;
    }

    void handleConfirmBreak();
  }, [active, autoStartBreakAfterFocus, studyState.phase, studyState.is_paused, studyState.id, studyState.cycle_index]);

  async function handleTogglePause() {
    try {
      setError(null);
      const requestId = beginStudyStateRequest();
      const nextState = studyState.is_paused ? await resumeStudyMode() : await pauseStudyMode();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice(nextState.is_paused ? (nextState.focus_enforcement_active ? `计时已暂停，${ruleModeLabel}仍在执行。` : '计时已暂停，前台规则已关闭。') : '已继续学习计时。');
      markStudyReminderSeen(nextState, syncDeviceId);
      if (!nextState.is_paused) {
        const refreshRequestId = beginStudyStateRequest();
        const refreshedState = await getStudyModeState();
        applyStudyStateIfCurrent(refreshedState, refreshRequestId);
        markStudyReminderSeen(refreshedState, syncDeviceId);
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

  function handleNormalExit() {
    setPendingConfirm({ kind: 'normalExit' });
  }

  async function confirmNormalExit() {
    try {
      setConfirmLoading(true);
      setError(null); setMonitorError(null); setLatestAppCheck(null);
      const requestId = beginStudyStateRequest();
      const nextState = await resetStudyMode();
      if (!applyStudyStateIfCurrent(nextState, requestId)) return;
      setNotice('已结束本次学习。');
      markStudyReminderSeen(nextState, syncDeviceId);
      await refreshDashboard();
      queueConfiguredSync();
      setIsChecklistDrawerOpen(false);
      setIsScheduleDrawerOpen(false);
      setPendingConfirm(null);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setConfirmLoading(false); }
  }

  async function handleCheckForeground() {
    const sessionId = studyState.current_session?.id;
    if (!sessionId) return;
    try {
      setMonitorError(null);
      const appCheck = await checkFocusForegroundApp(sessionId);
      setLatestAppCheck(appCheck);
      if (appCheck.match_result.matched_subject_id) await refreshStudyState();
    } catch (reason) { setMonitorError(reason instanceof Error ? reason.message : String(reason)); }
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
    if (!todayDraft.title.trim()) {
      setNotice(null);
      setError('今日任务需要先填写标题。');
      return;
    }
    await withChecklistRefresh(async () => {
      await createTodayPlanItem({ ...todayDraft, subjectId: todayDraft.subjectId ?? studyState.subject_id ?? null });
      setTodayDraft({ ...emptyTodayDraft, subjectId: studyState.subject_id ?? null });
      setShowTodayComposer(false);
    }, '今日任务已添加。');
  }

  async function handleSaveTodayEdit() {
    if (editingTodayId === null) return;
    if (!editingTodayDraft.title.trim()) {
      setNotice(null);
      setError('今日任务需要先填写标题。');
      return;
    }
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
    if (nextCompleted && item.source_task_id !== null) {
      setPendingConfirm({ kind: 'syncSourceCompletion', item });
      return;
    }
    await completeTodayItem(item, nextCompleted, false);
  }

  async function completeTodayItem(item: TodayPlanItem, completed: boolean, syncSourceCompletion: boolean) {
    await withChecklistRefresh(async () => { await completeTodayPlanItem(item.id, completed, syncSourceCompletion); }, completed ? '今日任务已完成。' : '今日任务已恢复为未完成。');
  }

  async function confirmSyncSourceCompletion(syncSourceCompletion: boolean) {
    if (pendingConfirm?.kind !== 'syncSourceCompletion') return;
    try {
      setConfirmLoading(true);
      const item = pendingConfirm.item;
      await completeTodayItem(item, true, syncSourceCompletion);
      setPendingConfirm(null);
    } finally {
      setConfirmLoading(false);
    }
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
    if (!scheduleDraft.title.trim()) {
      setNotice(null);
      setError('课表时间块需要先填写标题。');
      return;
    }
    if (scheduleDraft.endMinute <= scheduleDraft.startMinute) {
      setNotice(null);
      setError('课表时间块的结束时间必须晚于开始时间。');
      return;
    }
    const subjectId = studyState.subject_id ?? scheduleDraft.subjectId ?? null;
    await withChecklistRefresh(async () => {
      await createScheduleBlock({
        ...scheduleDraft,
        subjectId,
        categoryKey: scheduleSubjectCategory(subjectId),
      });
      setScheduleDraft(emptyScheduleDraft(formatDateKey()));
      await refreshScheduleData();
    }, '课表时间块已添加。');
  }

  async function handleQuickScheduleNextTask() {
    if (!nextTodayTask || isQuickSchedulingTask) return;
    try {
      setIsQuickSchedulingTask(true);
      setError(null);
      setNotice(null);
      const todayDate = formatDateKey();
      const latestScheduleData = await getSchedulePageData(todayDate);
      setScheduleData(latestScheduleData);
      const slot = findNextAvailableScheduleSlot(
        latestScheduleData.day_blocks,
        currentMinuteOfDay(new Date(localClockNow)),
        focusMinutes,
      );

      if (!slot) {
        setError(`今天没有足够的 ${formatDuration(focusMinutes * 60)} 空档，请打开今日课表手动调整。`);
        setIsChecklistDrawerOpen(false);
        setIsScheduleDrawerOpen(true);
        return;
      }

      await createScheduleBlockFromTodayItem(nextTodayTask.id, todayDate, slot.startMinute, slot.endMinute);
      await refreshScheduleData();
      setScheduleDraft(emptyScheduleDraft(todayDate));
      setIsChecklistDrawerOpen(false);
      setIsScheduleDrawerOpen(true);
      setNotice(`已把“${nextTodayTask.title}”安排到 ${formatMinuteOfDay(slot.startMinute)}-${formatMinuteOfDay(slot.endMinute)}。`);
      queueConfiguredSync('schedule_change');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setIsQuickSchedulingTask(false);
    }
  }

  async function handleDeleteScheduleBlock(blockId: number) {
    await withChecklistRefresh(async () => {
      await deleteScheduleBlock(blockId);
      await refreshScheduleData();
    }, '课表时间块已删除。');
  }

  async function handleStartScheduleBlock(block: ScheduleBlock) {
    if (isStartingStudy) return;
    try {
      setIsStartingStudy(true);
      setError(null); setMonitorError(null); setLatestAppCheck(null); setNotice(null);
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
    finally { setIsStartingStudy(false); }
  }

  const focusConfirmDialog = pendingConfirm?.kind === 'normalExit' ? (
    <ConfirmDialog
      cancelLabel="继续学习"
      confirmLabel="结束并记录"
      loading={confirmLoading}
      message="当前番茄会按已学习时长结束记录，专注历史和统计会同步更新。"
      onCancel={() => setPendingConfirm(null)}
      onConfirm={() => void confirmNormalExit()}
      open
      title="结束本次学习？"
    >
      <p>普通模式可以主动结束；强制模式不会显示这个退出入口。</p>
      <p>关闭窗口只会进入托盘并在后台继续计时，不等同于退出学习。</p>
    </ConfirmDialog>
  ) : pendingConfirm?.kind === 'syncSourceCompletion' ? (
    <ConfirmDialog
      cancelLabel="仅完成今日任务"
      confirmLabel="同步完成源待办"
      loading={confirmLoading}
      message="这条今日任务来自源待办。你可以只完成今天的进入任务，也可以同时把源待办标记完成。"
      onCancel={() => void confirmSyncSourceCompletion(false)}
      onConfirm={() => void confirmSyncSourceCompletion(true)}
      open
      title="同步完成源待办？"
    >
      <p>仅完成今日任务不会修改源待办，适合今天只是阶段性推进。</p>
    </ConfirmDialog>
  ) : null;

  const todayDrawer = (
    <TodayPlanDrawer
      compact
      dndContainerId={FOCUS_TODAY_CONTAINER_ID}
      dndIsOver={false}
      currentSubjectLabel={currentSubjectLabel}
      editingTodayDraft={editingTodayDraft}
      editingTodayId={editingTodayId}
      emptyDescription="可以在清单页拖入，也可以直接在这里补一条今天要推进的任务。"
      emptyTitle="今天还没有进入任务"
      isOpen={isChecklistDrawerOpen}
      items={checklistData?.today_items ?? []}
      onBeginEdit={beginEditTodayItem}
      onCancelEdit={() => setEditingTodayId(null)}
      onChangeEdit={(patch) => setEditingTodayDraft((current) => ({ ...(current ?? emptyTodayDraft), ...patch }))}
      onClose={() => setIsChecklistDrawerOpen(false)}
      onComplete={(item) => void handleCompleteTodayItem(item)}
      onCreate={() => void handleCreateTodayItem()}
      onDelete={(itemId) => void handleDeleteTodayItem(itemId)}
      onDraftChange={(patch) => setTodayDraft((current) => ({ ...current, ...patch }))}
      onRefresh={() => void refreshChecklistData()}
      onSaveEdit={() => void handleSaveTodayEdit()}
      onToggleComposer={() => setShowTodayComposer((current) => !current)}
      saving={checklistSaving}
      showComposer={showTodayComposer}
      sortable
      subtitle="今日队列"
      title="今日任务"
      getItemDragId={(item) => getTodaySortableId(item.id)}
      todayDate={checklistData?.today_date ?? ''}
      todayDraft={todayDraft}
      variant="drawer"
    />
  );

  const scheduleDrawer = (
    <ScheduleDrawer
      canStart={!active}
      isOpen={isScheduleDrawerOpen}
      data={scheduleData}
      draft={scheduleDraft}
      saving={checklistSaving || isQuickSchedulingTask || isStartingStudy}
      onClose={() => setIsScheduleDrawerOpen(false)}
      onRefresh={() => void refreshScheduleData()}
      onDraftChange={(patch) => setScheduleDraft((current) => ({ ...current, ...patch }))}
      onCreate={() => void handleCreateScheduleBlock()}
      onDelete={(blockId) => void handleDeleteScheduleBlock(blockId)}
      onStart={(block) => void handleStartScheduleBlock(block)}
    />
  );

  if (active) {
    return (
      <DndContext collisionDetection={closestCenter} onDragEnd={(event) => void handleTodayDrawerDragEnd(event)} sensors={sensors}>
        <>
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
                <label className={'focus-hud-card live-primary-toggle' + (isPrimaryDevice ? ' is-active' : '')}>
                  <span>{primaryStatusLabel}</span>
                  <input checked={isPrimaryDevice} disabled={!syncDeviceId} onChange={(event) => void handlePrimaryOwnerChange(event.target.checked)} role="switch" type="checkbox" />
                </label>
                <div className="live-badge"><span className={studyState.focus_enforcement_active ? 'live-dot on' : 'live-dot'} />{whitelistStatusLabel}</div>
              </div>
            </header>

            {(error || notice || monitorError) && <div className="focus-notice-stack">{error && <p className="alert error" role="alert">{error}</p>}{notice && <p aria-live="polite" className="alert success" role="status">{notice}</p>}{monitorError && <p className="alert error" role="alert">前台检测失败：{monitorError}</p>}</div>}

            <main className="focus-clock-zone">
              <p>{activeClockLabel}</p>
              <strong>{timerValue}</strong>
              <span>{buildPhaseMessage(studyState)}</span>
              <span>{activeModeMessage}</span>
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
                  <button aria-label="结束学习" className="focus-hud-card focus-command-button" onClick={handleNormalExit} title="结束学习" type="button">
                    <span className="focus-hud-icon"><Square size={14} /></span>
                    <span className="focus-hud-copy">
                      <span>结束学习</span>
                      <strong>普通模式</strong>
                    </span>
                  </button>
                )}
              </div>
            </footer>

            {todayDrawer}
            {scheduleDrawer}
          </section>
          {focusConfirmDialog}
        </>
      </DndContext>
    );
  }

  return (
    <DndContext collisionDetection={closestCenter} onDragEnd={(event) => void handleTodayDrawerDragEnd(event)} sensors={sensors}>
      <>
    <section className={'page-shell focus-prepare-shell' + ((isChecklistDrawerOpen || isScheduleDrawerOpen) ? ' is-drawer-open' : '')}>
      <header className="page-header prepare-header">
        <div><p className="eyebrow">Focus Ritual</p><h2>进入学习模式</h2><p>设定本次学习长度、番茄节奏和休息规则。开始后界面会切换为极简番茄钟，配置入口自动锁定。</p></div>
        <div className={'phase-badge phase-' + studyState.phase}><span>{phaseLabel[studyState.phase]}</span><strong>{studyState.phase === 'finished' ? '已收束' : '待命'}</strong></div>
      </header>
      {error && <p className="alert error" role="alert">{error}</p>}{notice && <p aria-live="polite" className="alert success" role="status">{notice}</p>}{monitorError && <p className="alert error" role="alert">前台检测失败：{monitorError}</p>}
      <LearningHub
        completedTodayItems={completedTodayItems}
        desktopReady={desktopReady}
        isSchedulingTask={isQuickSchedulingTask}
        nextScheduleBlock={scheduleRecommendationBlock}
        nextTask={nextTodayTask}
        onOpenSchedule={handleToggleScheduleDrawer}
        onOpenTodayTasks={handleToggleChecklistDrawer}
        onQuickScheduleTask={() => void handleQuickScheduleNextTask()}
        pendingTodayCount={pendingTodayCount}
        quickScheduleDisabled={quickScheduleDisabled}
        scheduledBlockCount={scheduleData?.day_blocks.length ?? 0}
        scheduleBlockMeta={scheduleRecommendationMeta}
        todayTaskCount={todayTaskCount}
      />
      <div className="prepare-grid">
        <section className="start-console">
          <div className="timer-orbit"><span>下一轮专注</span><strong>{formatSeconds(focusMinutes * 60)}</strong><p>{selectedSubjectName ?? '未指定科目'} / {mode === 'strict' ? '强制模式' : '普通模式'}</p></div>
          <div className="console-facts"><CoreFact label="学习模式" value={formatDuration(studyMinutes * 60)} /><CoreFact label="番茄时长" value={formatDuration(focusMinutes * 60)} /><CoreFact label="短休息" value={formatDuration(breakMinutes * 60)} /><CoreFact label="长休息" value={formatDuration(longBreakMinutes * 60) + ' / ' + longBreakInterval + ' 轮'} /></div>
          <button aria-busy={isStartingStudy} className="start-ritual-button" disabled={isStartingStudy} onClick={handleStart} type="button">
            <Play size={22} />
            {isStartingStudy ? '正在开始' : '开始学习'}
          </button>
        </section>
        <aside className="control-panel">
          <div className="panel-title"><div><p className="eyebrow">Plan</p><h3>本次节奏</h3></div><BookOpen size={20} /></div>
          <div className="preset-grid">
            <PresetSelect label="学习模式" items={studyPresetMinutes} selected={studyMinutes} suffix="m" onSelect={setStudyMinutes} />
            <PresetSelect label="番茄时长" items={focusPresetMinutes} selected={focusMinutes} suffix="m" onSelect={setFocusMinutes} />
            <PresetSelect label="短休时长" items={breakPresetMinutes} selected={breakMinutes} suffix="m" onSelect={setBreakMinutes} />
            <PresetSelect label="长休时长" items={longBreakPresetMinutes} selected={longBreakMinutes} suffix="m" onSelect={setLongBreakMinutes} />
            <PresetSelect label="长休间隔" items={longBreakIntervalPresets} selected={longBreakInterval} suffix="轮" onSelect={setLongBreakInterval} />
          </div>
          <label className="field-block"><span>科目</span><select className="select-input" disabled={subjects.length === 0} onChange={(event) => setSelectedSubjectId(event.target.value ? Number(event.target.value) : null)} value={selectedSubjectId ?? ''}><option value="">不指定</option>{subjects.map((subject) => <option disabled={!subject.enabled} key={subject.id} value={subject.id}>{subject.name}</option>)}</select></label>
          <div className="segmented-control"><button className={mode === 'normal' ? 'active' : ''} onClick={() => setMode('normal')} type="button">普通模式</button><button className={mode === 'strict' ? 'active' : ''} onClick={() => setMode('strict')} type="button">强制模式</button></div>
          <label className="capability-row focus-whitelist-toggle focus-primary-toggle">
            <span>以当前设备为主</span>
            <input checked={isPrimaryDevice} disabled={!syncDeviceId} onChange={(event) => void handlePrimaryOwnerChange(event.target.checked)} role="switch" type="checkbox" />
          </label>
          <p className="focus-primary-hint">{primaryStatusLabel}。主端可主导专注状态，另一端不能回退本端进度。</p>
          {mode === 'normal' && (
            <label className="capability-row focus-whitelist-toggle">
              <span>启用前台规则</span>
              <input checked={normalWhitelistEnabled} onChange={(event) => setNormalWhitelistEnabled(event.target.checked)} role="switch" type="checkbox" />
            </label>
          )}
        </aside>
      </div>
      <div className="dashboard-strip">
        <section className="soft-panel"><div className="panel-title"><div><p className="eyebrow">Today</p><h3>今日状态</h3></div><CheckCircle2 size={20} /></div><div className="stats-grid four"><Metric icon={Timer} label="今日" value={formatDuration(stats?.today_seconds ?? 0)} /><Metric icon={Timer} label="本周" value={formatDuration(stats?.week_seconds ?? 0)} /><Metric icon={Timer} label="本月" value={formatDuration(stats?.month_seconds ?? 0)} /><Metric icon={ShieldCheck} label="拦截" value={String(stats?.interruption_count ?? 0)} /></div></section>
        <section className="soft-panel"><div className="panel-title"><div><p className="eyebrow">Monitor</p><h3>{ruleModeLabel}状态</h3></div><Gauge size={20} /></div><div className="monitor-callout"><Leaf size={18} /><div><strong>准备就绪</strong><p>{mode === 'normal' && !normalWhitelistEnabled ? '普通模式本次不执行前台规则。' : `开始学习后会持续检查前台窗口，并按${ruleModeLabel}${ruleActionLabel}。`}</p></div></div></section>
        <section className="soft-panel history-panel"><div className="panel-title"><div><p className="eyebrow">History</p><h3>最近记录</h3></div><BellRing size={20} /></div>{history.length === 0 ? <div className="empty-state compact">还没有专注记录。</div> : <div className="compact-history">{history.slice(0, 4).map((session) => <article className="list-row compact-row" key={session.id}><div><strong>{session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未知科目' : '未指定科目'}</strong><p>{formatSessionTimeRange(session)}</p></div><div className="history-meta"><span>{sessionStatusLabel(session.status)}</span><strong>{formatDuration(session.actual_seconds || session.planned_seconds)}</strong></div></article>)}</div>}</section>
      </div>
    </section>
    {todayDrawer}
    {scheduleDrawer}
    {focusConfirmDialog}
      </>
    </DndContext>
  );
}

function breakKindLabel(kind: StudyModeState['break_kind']) { return studyBreakKindLabel(kind); }
function nextBreakLabel(studyState: StudyModeState) { return nextStudyBreakLabel(studyState); }
function buildPhaseMessage(studyState: StudyModeState) {
  if (studyState.is_paused) return studyState.focus_enforcement_active ? '计时暂停，前台规则仍在强制执行' : '计时暂停，前台规则已关闭';
  if (studyState.phase === 'focus') return '第 ' + studyState.cycle_index + ' 轮番茄钟进行中';
  if (studyState.phase === 'awaiting_break') return '本轮已到点，确认后进入 ' + nextBreakLabel(studyState);
  if (studyState.phase === 'break') return breakKindLabel(studyState.break_kind) + '进行中';
  if (studyState.phase === 'finished') return '学习模式已完成';
  if (studyState.phase === 'emergency_exited') return '历史退出状态';
  return '设置节奏后开始';
}
function buildActiveModeMessage(studyState: StudyModeState, ruleModeLabel: string) {
  if (studyState.mode === 'strict') return `强制模式：${ruleModeLabel}强制执行，不能从这里手动结束；关闭窗口会进入托盘，后台继续计时。`;
  const whitelist = studyState.whitelist_enabled ? `本次执行${ruleModeLabel}` : '本次不执行前台规则';
  return '普通模式：' + whitelist + '，可手动结束；退出会按已学习时长记录，关闭窗口只会进入托盘并后台继续计时。';
}
function foregroundSummary(check: FocusAppCheck) {
  const mediaPath = check.match_result.potplayer_media_path || check.foreground_app.potplayer_media_path;
  const detail = mediaPath || check.foreground_app.window_title || '无窗口标题';
  return (check.match_result.allowed ? '已放行' : '已拦截') + ' / ' + check.foreground_app.process_name + ' / ' + detail;
}
function getTodaySortableId(itemId: number) { return 'today:' + itemId; }
function Metric({ icon: Icon, label, value }: { icon: typeof Timer; label: string; value: string }) { return <article className="metric-card"><Icon size={18} /><span>{label}</span><strong>{value}</strong></article>; }
function CoreFact({ label, value }: { label: string; value: string }) { return <article className="core-fact"><span>{label}</span><strong>{value}</strong></article>; }
function NumberField({ label, onChange, value }: { label: string; onChange: (value: number) => void; value: number }) { return <label className="field-block"><span>{label}</span><input className="number-input" min={1} onChange={(event) => onChange(Number(event.target.value) || 1)} type="number" value={value} /></label>; }
function PresetSelect({ label, items, onSelect, selected, suffix }: { label: string; items: number[]; onSelect: (value: number) => void; selected: number; suffix: string }) {
  return (
    <label className="preset-row">
      <span>{label}</span>
      <select className="select-input preset-select" onChange={(event) => onSelect(Number(event.target.value) || items[0] || 0)} value={selected}>
        {items.map((value) => (
          <option key={`${label}-${value}`} value={value}>
            {value}
            {suffix}
          </option>
        ))}
      </select>
    </label>
  );
}

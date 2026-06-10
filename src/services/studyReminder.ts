import type { StudyModeState } from '../types/focus';

export type StudyReminderPayload = {
  title: string;
  body: string;
  wakeWindow?: boolean;
};

type ReminderScopeRef = {
  current: string | null;
};

const REMINDER_STORAGE_KEY = 'focus.notifiedPhaseKeys.v2';
const FINISHED_REMINDER_STALE_MS = 5 * 60 * 1000;

export function formatStudyDuration(seconds: number) {
  if (seconds <= 0) return '0 分钟';
  if (seconds < 3600) return Math.round(seconds / 60) + ' 分钟';
  const hours = seconds / 3600;
  return (Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)) + ' 小时';
}

export function studyBreakKindLabel(kind: StudyModeState['break_kind']) {
  return kind === 'long' ? '长休息' : '短休息';
}

export function nextStudyBreakLabel(studyState: StudyModeState) {
  return studyBreakKindLabel(studyState.break_kind) + ' ' + formatStudyDuration(studyState.effective_break_seconds || studyState.break_seconds);
}

export function isFinishedStudyMode(studyState: StudyModeState) {
  return studyState.status === 'finished' || studyState.phase === 'finished';
}

export function isStaleFinishedStudyReminder(studyState: StudyModeState, now = Date.now()) {
  if (!isFinishedStudyMode(studyState) || !studyState.ended_at) {
    return false;
  }

  const endedAt = new Date(studyState.ended_at).getTime();
  if (!Number.isFinite(endedAt)) {
    return false;
  }

  return now - endedAt > FINISHED_REMINDER_STALE_MS;
}

export function buildStudyReminder(studyState: StudyModeState): StudyReminderPayload | null {
  if (studyState.status === 'active' && studyState.phase === 'focus') {
    return {
      title: studyState.cycle_index > 1 ? '下一轮番茄钟开始' : '番茄钟开始',
      body: '第 ' + studyState.cycle_index + ' 轮开始，专注 ' + formatStudyDuration(studyState.focus_seconds) + '。',
    };
  }

  if (studyState.status === 'active' && studyState.phase === 'awaiting_break') {
    return {
      title: '番茄钟结束',
      body: '本轮已经到点。确认后进入 ' + nextStudyBreakLabel(studyState) + '；未确认前学习时间继续累计。',
      wakeWindow: true,
    };
  }

  if (studyState.status === 'active' && studyState.phase === 'break') {
    return {
      title: studyBreakKindLabel(studyState.break_kind) + '开始',
      body: formatStudyDuration(studyState.effective_break_seconds) + ' 后自动进入下一轮番茄钟。',
    };
  }

  if (isFinishedStudyMode(studyState)) {
    return {
      title: '学习模式完成',
      body: '本次学习已完成，共累计 ' + formatStudyDuration(studyState.study_elapsed_seconds) + '。',
      wakeWindow: true,
    };
  }

  return null;
}

export function registerStudyReminderScope(studyState: StudyModeState, scopeRef: ReminderScopeRef) {
  const scope = studyReminderScope(studyState);
  if (scopeRef.current === scope) return;
  scopeRef.current = scope;
  if (scope === 'idle') return;

  const keys = loadStudyReminderKeys();
  for (const key of Array.from(keys)) {
    const parts = key.split(':');
    if (parts[1] !== scope) keys.delete(key);
  }
  saveStudyReminderKeys(keys);
}

export function resetStudyReminderScope(studyState: StudyModeState, scopeRef: ReminderScopeRef) {
  scopeRef.current = null;
  registerStudyReminderScope(studyState, scopeRef);
}

export function markStudyReminderSeen(studyState: StudyModeState, deviceId?: string | null) {
  const reminder = buildStudyReminder(studyState);
  if (!reminder) return false;

  const keys = loadStudyReminderKeys();
  const key = studyReminderKey(studyState, deviceId);
  if (keys.has(key)) {
    return false;
  }

  keys.add(key);
  saveStudyReminderKeys(keys);
  return true;
}

function studyReminderScope(studyState: StudyModeState) {
  return String(studyState.id ?? 'idle');
}

function studyReminderKey(studyState: StudyModeState, deviceId?: string | null) {
  return [deviceId ?? 'local', studyState.id ?? 'idle', studyState.phase, studyState.cycle_index, studyState.break_kind].join(':');
}

function loadStudyReminderKeys() {
  try {
    const raw = window.localStorage.getItem(REMINDER_STORAGE_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : []);
  } catch {
    return new Set<string>();
  }
}

function saveStudyReminderKeys(keys: Set<string>) {
  try {
    window.localStorage.setItem(REMINDER_STORAGE_KEY, JSON.stringify(Array.from(keys).slice(-200)));
  } catch {}
}

import { useEffect, useMemo, useState } from 'react';
import {
  Activity,
  BellRing,
  BookOpen,
  CheckCircle2,
  Coffee,
  Gauge,
  Leaf,
  Pause,
  Play,
  RefreshCw,
  ShieldCheck,
  Timer,
} from 'lucide-react';
import {
  confirmStudyBreak,
  getFocusStatsSummary,
  getStudyModeState,
  listFocusSessions,
  listSubjects,
  pauseStudyMode,
  resumeStudyMode,
  startStudyMode,
  updateStudyModeSubject,
} from '../services/focusApi';
import { notifyStudyReminder } from '../services/alertApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { getAppSettings } from '../services/settingsApi';
import type { FocusMode, FocusSession, FocusStatsSummary, StudyModePhase, StudyModeState, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';

const studyPresetMinutes = [60, 120, 180, 240];
const focusPresetMinutes = [25, 45, 60, 90];
const breakPresetMinutes = [5, 10, 15, 20];
const longBreakPresetMinutes = [10, 15, 20, 30];
const longBreakIntervalPresets = [2, 3, 4, 6];

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
  is_paused: false,
};

const phaseLabel: Record<StudyModePhase, string> = {
  idle: '待开始',
  focus: '专注中',
  awaiting_break: '等待休息确认',
  break: '休息中',
  finished: '已完成',
  emergency_exited: '已退出（历史）',
};

const phaseSubLabel: Record<StudyModePhase, string> = {
  idle: '准备进入学习模式',
  focus: '保持学习，后台强制执行白名单',
  awaiting_break: '本轮已到点，确认后开始休息',
  break: '休息结束后自动进入下一轮',
  finished: '本次学习已写入记录',
  emergency_exited: '历史退出状态',
};

let reminderBaselineKey: string | null = null;

function formatSeconds(totalSeconds: number) {
  const safeSeconds = Math.max(totalSeconds, 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60).toString().padStart(2, '0');
  const seconds = Math.floor(safeSeconds % 60).toString().padStart(2, '0');

  if (hours > 0) {
    return `${hours}:${minutes}:${seconds}`;
  }

  return `${minutes}:${seconds}`;
}

function formatDuration(seconds: number) {
  if (seconds <= 0) {
    return '0 分钟';
  }

  if (seconds < 3600) {
    return `${Math.round(seconds / 60)} 分钟`;
  }

  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return '暂无';
  }

  return new Date(value).toLocaleString();
}

function sessionStatusLabel(status: string) {
  const labels: Record<string, string> = {
    running: '进行中',
    finished: '已完成',
    interrupted: '已中断',
    emergency_exited: '已退出（历史）',
  };

  return labels[status] ?? status;
}

export default function FocusPage() {
  const [studyMinutes, setStudyMinutes] = useState(120);
  const [focusMinutes, setFocusMinutes] = useState(25);
  const [breakMinutes, setBreakMinutes] = useState(5);
  const [longBreakMinutes, setLongBreakMinutes] = useState(15);
  const [longBreakInterval, setLongBreakInterval] = useState(4);
  const [mode, setMode] = useState<FocusMode>('normal');
  const [studyState, setStudyState] = useState<StudyModeState>(idleStudyState);
  const [history, setHistory] = useState<FocusSession[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [selectedSubjectId, setSelectedSubjectId] = useState<number | null>(null);
  const [stats, setStats] = useState<FocusStatsSummary | null>(null);
  const [latestAppCheck, setLatestAppCheck] = useState<FocusAppCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const active = studyState.status === 'active';
  const controlsDisabled = active;
  const canPause = active && (studyState.phase === 'focus' || studyState.phase === 'awaiting_break');
  const currentSession = studyState.current_session;
  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);
  const displayedSubjectId = active ? studyState.subject_id : selectedSubjectId;
  const selectedSubjectName = displayedSubjectId ? subjectNameMap.get(displayedSubjectId) : null;
  const timerValue = studyState.phase === 'idle'
    ? formatSeconds(focusMinutes * 60)
    : studyState.phase === 'awaiting_break'
      ? formatSeconds(studyState.phase_elapsed_seconds)
      : formatSeconds(studyState.phase_remaining_seconds);
  const activeClockLabel = studyState.is_paused
    ? '已暂停'
    : studyState.phase === 'awaiting_break' ? '已继续学习' : '阶段倒计时';
  const totalProgress = studyState.planned_seconds > 0
    ? Math.min((studyState.study_elapsed_seconds / studyState.planned_seconds) * 100, 100)
    : 0;
  const phaseProgress = currentPhaseTotalSeconds(studyState) > 0
    ? Math.min((studyState.phase_elapsed_seconds / currentPhaseTotalSeconds(studyState)) * 100, 100)
    : studyState.phase === 'awaiting_break' ? 100 : 0;

  useEffect(() => {
    void initializePage();
  }, []);

  useEffect(() => {
    if (!active) {
      return;
    }

    const intervalId = window.setInterval(() => {
      void refreshStudyState();
    }, 1000);

    return () => window.clearInterval(intervalId);
  }, [active]);

  useEffect(() => {
    if (!studyState.focus_enforcement_active || currentSession === null) {
      return;
    }

    const checkForeground = async () => {
      try {
        setMonitorError(null);
        const result = await checkFocusForegroundApp(currentSession.id);
        setLatestAppCheck(result);
      } catch (reason) {
        setMonitorError(reason instanceof Error ? reason.message : String(reason));
      }
    };

    void checkForeground();
    const intervalId = window.setInterval(() => void checkForeground(), 5000);
    return () => window.clearInterval(intervalId);
  }, [currentSession?.id, studyState.focus_enforcement_active]);

  useEffect(() => {
    if (reminderBaselineKey === null) {
      reminderBaselineKey = reminderKey(studyState);
      return;
    }

    const key = reminderKey(studyState);
    if (key === reminderBaselineKey) {
      return;
    }

    reminderBaselineKey = key;
    const reminder = buildReminder(studyState);
    if (reminder) {
      void notifyStudyReminder(reminder);
    }
  }, [studyState.id, studyState.phase, studyState.status, studyState.cycle_index]);

  async function initializePage() {
    try {
      setError(null);
      const [settings, state] = await Promise.all([getAppSettings(), getStudyModeState()]);
      reminderBaselineKey = reminderKey(state);
      setStudyState(state);

      if (state.status !== 'active') {
        setStudyMinutes(settings.default_study_minutes);
        setFocusMinutes(settings.default_focus_minutes);
        setBreakMinutes(settings.break_minutes);
        setLongBreakMinutes(settings.long_break_minutes);
        setLongBreakInterval(settings.long_break_interval);
        setMode(settings.default_focus_mode);
      } else {
        setStudyMinutes(Math.max(1, Math.round(state.planned_seconds / 60)));
        setFocusMinutes(Math.max(1, Math.round(state.focus_seconds / 60)));
        setBreakMinutes(Math.max(1, Math.round(state.break_seconds / 60)));
        setLongBreakMinutes(Math.max(1, Math.round(state.long_break_seconds / 60)));
        setLongBreakInterval(Math.max(1, state.long_break_interval));
        setMode(state.mode);
      }

      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshDashboard() {
    try {
      const [historyData, subjectsData, statsData] = await Promise.all([
        listFocusSessions(),
        listSubjects(),
        getFocusStatsSummary(),
      ]);
      setHistory(historyData);
      setSubjects(subjectsData);
      setStats(statsData);
      setSelectedSubjectId((current) => {
        if (current !== null && subjectsData.some((subject) => subject.id === current)) {
          return current;
        }

        return subjectsData[0]?.id ?? null;
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshStudyState() {
    try {
      const nextState = await getStudyModeState();
      setStudyState(nextState);

      if (nextState.status !== 'active') {
        await refreshDashboard();
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleStart() {
    try {
      setError(null);
      setMonitorError(null);
      setLatestAppCheck(null);
      setNotice(null);
      const nextState = await startStudyMode(
        studyMinutes * 60,
        focusMinutes * 60,
        breakMinutes * 60,
        longBreakMinutes * 60,
        longBreakInterval,
        mode,
        selectedSubjectId,
      );
      setStudyState(nextState);
      setNotice('学习模式已开始。窗口关闭后会进入托盘，后台继续计时并执行白名单。');
      reminderBaselineKey = reminderKey(nextState);
      void notifyStudyReminder({
        title: '学习模式已开始',
        body: `第 ${nextState.cycle_index} 轮番茄钟开始，专注 ${formatDuration(nextState.focus_seconds)}。`,
      });
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleConfirmBreak() {
    try {
      setError(null);
      setMonitorError(null);
      setLatestAppCheck(null);
      const nextState = await confirmStudyBreak();
      setStudyState(nextState);
      setNotice(`${breakKindLabel(nextState.break_kind)}已开始。休息结束后会自动进入下一轮番茄钟。`);
      reminderBaselineKey = reminderKey(nextState);
      void notifyStudyReminder({
        title: `${breakKindLabel(nextState.break_kind)}开始`,
        body: `休息 ${formatDuration(nextState.effective_break_seconds)}，到点后自动进入下一轮番茄钟。`,
      });
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleTogglePause() {
    try {
      setError(null);
      const nextState = studyState.is_paused ? await resumeStudyMode() : await pauseStudyMode();
      setStudyState(nextState);
      setNotice(nextState.is_paused ? '已暂停计时，白名单仍在执行。' : '已继续学习计时。');
      reminderBaselineKey = reminderKey(nextState);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleActiveSubjectChange(value: string) {
    try {
      setError(null);
      const subjectId = value ? Number(value) : null;
      const nextState = await updateStudyModeSubject(subjectId);
      setStudyState(nextState);
      setNotice('本次学习科目已更新。');
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  if (active) {
    return (
      <section className={`page-shell pomodoro-shell phase-${studyState.phase}${studyState.is_paused ? ' is-paused' : ''}`}>
        {error && <p className="alert error">{error}</p>}
        {notice && <p className="alert success">{notice}</p>}
        {monitorError && <p className="alert error">前台检测失败：{monitorError}</p>}

        <div className="pomodoro-focus">
          <div className="pomodoro-topline">
            <span>{studyState.is_paused ? '已暂停' : phaseLabel[studyState.phase]}</span>
            <span>{selectedSubjectName ?? '未指定科目'}</span>
          </div>

          <div className="pomodoro-clock">
            <p>{activeClockLabel}</p>
            <strong>{timerValue}</strong>
            <span>{buildPhaseMessage(studyState)}</span>
          </div>

          <div className="pomodoro-progress">
            <ProgressBar label="学习总进度" value={totalProgress} />
            <ProgressBar label="当前阶段" value={phaseProgress} accent />
          </div>

          <div className="pomodoro-core-grid">
            <CoreFact label="轮次" value={`第 ${studyState.cycle_index} 轮`} />
            <CoreFact label="剩余学习" value={formatSeconds(studyState.study_remaining_seconds)} />
            <CoreFact label="休息规则" value={nextBreakLabel(studyState)} />
            <CoreFact label="白名单" value={studyState.focus_enforcement_active ? '强制执行中' : '休息中暂停'} />
          </div>

          <div className="pomodoro-live-controls">
            <label className="pomodoro-subject-control">
              <span>当前科目</span>
              <select
                className="select-input"
                disabled={subjects.length === 0}
                onChange={(event) => void handleActiveSubjectChange(event.target.value)}
                value={studyState.subject_id ?? ''}
              >
                <option value="">未指定</option>
                {subjects.map((subject) => (
                  <option key={subject.id} value={subject.id}>{subject.name}</option>
                ))}
              </select>
            </label>
          </div>

          <div className="pomodoro-actions">
            {studyState.phase === 'awaiting_break' ? (
              <button className="primary-action" disabled={studyState.is_paused} onClick={handleConfirmBreak} type="button">
                <Coffee size={18} />
                确认开始{breakKindLabel(studyState.break_kind)}
              </button>
            ) : (
              <button className="secondary-action" onClick={refreshStudyState} type="button">
                <RefreshCw size={18} />
                刷新状态
              </button>
            )}
            {canPause && (
              <button className={studyState.is_paused ? 'primary-action pause-action' : 'secondary-action pause-action'} onClick={handleTogglePause} type="button">
                {studyState.is_paused ? <Play size={18} /> : <Pause size={18} />}
                {studyState.is_paused ? '继续' : '暂停'}
              </button>
            )}
          </div>

          <div className="pomodoro-footer">
            <span>{phaseSubLabel[studyState.phase]}</span>
            <span>{latestAppCheck ? foregroundSummary(latestAppCheck) : '前台监控等待下一次检查'}</span>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="page-shell focus-ritual-shell">
      <header className="ritual-header">
        <div>
          <p className="eyebrow">沉浸番茄钟</p>
          <h2>准备进入学习模式</h2>
          <p>设置本次学习时长、番茄节奏和休息规则。开始后页面会切换成极简番茄钟，设置与白名单会自动锁定。</p>
        </div>
        <div className={`phase-badge phase-${studyState.phase}`}>
          <span>{phaseLabel[studyState.phase]}</span>
          <strong>{studyState.phase === 'finished' ? '已完成' : '待命'}</strong>
        </div>
      </header>

      {error && <p className="alert error">{error}</p>}
      {notice && <p className="alert success">{notice}</p>}
      {monitorError && <p className="alert error">前台检测失败：{monitorError}</p>}

      <div className="ritual-grid">
        <section className="ritual-stage">
          <div className="ritual-orbit">
            <span>下一轮专注</span>
            <strong>{formatSeconds(focusMinutes * 60)}</strong>
            <p>{selectedSubjectName ?? '未指定科目'} · {mode === 'strict' ? '强制模式' : '普通模式'}</p>
          </div>

          <div className="ritual-summary">
            <CoreFact label="学习模式" value={formatDuration(studyMinutes * 60)} />
            <CoreFact label="番茄时长" value={formatDuration(focusMinutes * 60)} />
            <CoreFact label="短休息" value={formatDuration(breakMinutes * 60)} />
            <CoreFact label="长休息" value={`${formatDuration(longBreakMinutes * 60)} / ${longBreakInterval} 轮`} />
          </div>

          <button className="start-ritual-button" disabled={controlsDisabled} onClick={handleStart} type="button">
            <Play size={22} />
            开始学习
          </button>
        </section>

        <aside className="ritual-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Plan</p>
              <h3>本次节奏</h3>
            </div>
            <BookOpen size={20} />
          </div>

          <div className="ritual-fields">
            <NumberField disabled={controlsDisabled} label="学习模式" onChange={setStudyMinutes} value={studyMinutes} />
            <NumberField disabled={controlsDisabled} label="番茄钟" onChange={setFocusMinutes} value={focusMinutes} />
            <NumberField disabled={controlsDisabled} label="短休息" onChange={setBreakMinutes} value={breakMinutes} />
            <NumberField disabled={controlsDisabled} label="长休息" onChange={setLongBreakMinutes} value={longBreakMinutes} />
          </div>

          <PresetStrip disabled={controlsDisabled} items={studyPresetMinutes} prefix="学习 " selected={studyMinutes} suffix="m" onSelect={setStudyMinutes} />
          <PresetStrip disabled={controlsDisabled} items={focusPresetMinutes} prefix="番茄 " selected={focusMinutes} suffix="m" onSelect={setFocusMinutes} />
          <PresetStrip disabled={controlsDisabled} items={breakPresetMinutes} prefix="短休 " selected={breakMinutes} suffix="m" onSelect={setBreakMinutes} />
          <PresetStrip disabled={controlsDisabled} items={longBreakPresetMinutes} prefix="长休 " selected={longBreakMinutes} suffix="m" onSelect={setLongBreakMinutes} />
          <PresetStrip disabled={controlsDisabled} items={longBreakIntervalPresets} prefix="每 " selected={longBreakInterval} suffix=" 轮长休" onSelect={setLongBreakInterval} />

          <label className="field-block">
            <span>科目</span>
            <select
              className="select-input"
              disabled={controlsDisabled || subjects.length === 0}
              onChange={(event) => setSelectedSubjectId(event.target.value ? Number(event.target.value) : null)}
              value={selectedSubjectId ?? ''}
            >
              <option value="">不指定</option>
              {subjects.map((subject) => (
                <option key={subject.id} value={subject.id}>{subject.name}</option>
              ))}
            </select>
          </label>

          <div className="segmented-control">
            <button className={mode === 'normal' ? 'active' : ''} disabled={controlsDisabled} onClick={() => setMode('normal')} type="button">
              普通模式
            </button>
            <button className={mode === 'strict' ? 'active' : ''} disabled={controlsDisabled} onClick={() => setMode('strict')} type="button">
              强制模式
            </button>
          </div>
        </aside>
      </div>

      <div className="ritual-overview">
        <section className="soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Today</p>
              <h3>今日状态</h3>
            </div>
            <CheckCircle2 size={20} />
          </div>
          <div className="stats-grid four">
            <Metric icon={Timer} label="今日" value={formatDuration(stats?.today_seconds ?? 0)} />
            <Metric icon={Timer} label="本周" value={formatDuration(stats?.week_seconds ?? 0)} />
            <Metric icon={Timer} label="本月" value={formatDuration(stats?.month_seconds ?? 0)} />
            <Metric icon={ShieldCheck} label="拦截" value={`${stats?.interruption_count ?? 0}`} />
          </div>
        </section>

        <section className="soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Monitor</p>
              <h3>白名单状态</h3>
            </div>
            <Gauge size={20} />
          </div>
          <div className="ritual-monitor">
            <Leaf size={18} />
            <div>
              <strong>{active ? '运行中' : '准备就绪'}</strong>
              <p>{latestAppCheck ? foregroundSummary(latestAppCheck) : '开始学习后会持续检查前台窗口，并关闭非白名单软件或网站。'}</p>
            </div>
          </div>
        </section>

        <section className="soft-panel history-soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">History</p>
              <h3>最近记录</h3>
            </div>
            <BellRing size={20} />
          </div>
          {history.length === 0 ? (
            <div className="empty-state compact">还没有专注记录。</div>
          ) : (
            <div className="compact-history">
              {history.slice(0, 4).map((session) => (
                <article className="list-row compact-row" key={session.id}>
                  <div>
                    <strong>{session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未知科目' : '未指定科目'}</strong>
                    <p>{formatDateTime(session.started_at)}</p>
                  </div>
                  <div className="history-meta">
                    <span>{sessionStatusLabel(session.status)}</span>
                    <strong>{formatDuration(session.actual_seconds || session.planned_seconds)}</strong>
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>
      </div>
    </section>
  );
}

function currentPhaseTotalSeconds(studyState: StudyModeState) {
  if (studyState.phase === 'focus') {
    return studyState.focus_seconds;
  }

  if (studyState.phase === 'break') {
    return studyState.effective_break_seconds;
  }

  return 0;
}

function breakKindLabel(kind: StudyModeState['break_kind']) {
  return kind === 'long' ? '长休息' : '短休息';
}

function nextBreakLabel(studyState: StudyModeState) {
  const seconds = studyState.effective_break_seconds || studyState.break_seconds;
  return `${breakKindLabel(studyState.break_kind)} ${formatDuration(seconds)}`;
}

function buildPhaseMessage(studyState: StudyModeState) {
  if (studyState.is_paused) {
    return '计时已暂停，白名单仍在强制执行';
  }

  if (studyState.phase === 'focus') {
    return `第 ${studyState.cycle_index} 轮番茄钟进行中`;
  }

  if (studyState.phase === 'awaiting_break') {
    return `本轮已到点，确认后进入${nextBreakLabel(studyState)}`;
  }

  if (studyState.phase === 'break') {
    return `${breakKindLabel(studyState.break_kind)}中`;
  }

  if (studyState.phase === 'finished') {
    return '学习模式已完成';
  }

  if (studyState.phase === 'emergency_exited') {
    return '历史退出状态';
  }

  return '设置节奏后开始';
}

function reminderKey(studyState: StudyModeState) {
  return [
    studyState.id ?? 'idle',
    studyState.status,
    studyState.phase,
    studyState.cycle_index,
    studyState.break_kind,
    studyState.is_paused ? 'paused' : 'running',
  ].join(':');
}

function buildReminder(studyState: StudyModeState) {
  if (studyState.status === 'active' && studyState.phase === 'focus') {
    return {
      title: studyState.cycle_index > 1 ? '下一轮番茄钟开始' : '番茄钟开始',
      body: `第 ${studyState.cycle_index} 轮开始，专注 ${formatDuration(studyState.focus_seconds)}。`,
    };
  }

  if (studyState.status === 'active' && studyState.phase === 'awaiting_break') {
    return {
      title: '番茄钟结束',
      body: `本轮已经到点。确认后进入${nextBreakLabel(studyState)}；未确认前学习时间继续累计。`,
    };
  }

  if (studyState.status === 'active' && studyState.phase === 'break') {
    return {
      title: `${breakKindLabel(studyState.break_kind)}开始`,
      body: `${formatDuration(studyState.effective_break_seconds)} 后自动进入下一轮番茄钟。`,
    };
  }

  if (studyState.status === 'finished' || studyState.phase === 'finished') {
    return {
      title: '学习模式完成',
      body: `本次学习已完成，共累计 ${formatDuration(studyState.study_elapsed_seconds)}。`,
    };
  }

  return null;
}

function foregroundSummary(check: FocusAppCheck) {
  const action = check.match_result.allowed ? '已放行' : '已拦截';
  const title = check.foreground_app.window_title || '无窗口标题';
  return `${action} · ${check.foreground_app.process_name} · ${title}`;
}

function ProgressBar({ accent = false, label, value }: { accent?: boolean; label: string; value: number }) {
  return (
    <div>
      <div className="progress-label">
        <span>{label}</span>
        <strong>{Math.round(value)}%</strong>
      </div>
      <div className={accent ? 'progress-track accent' : 'progress-track'}>
        <i style={{ width: `${value}%` }} />
      </div>
    </div>
  );
}

function Metric({ icon: Icon, label, value }: { icon: typeof Timer; label: string; value: string }) {
  return (
    <article className="metric-card">
      <Icon size={18} />
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function CoreFact({ label, value }: { label: string; value: string }) {
  return (
    <article className="core-fact">
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function NumberField({
  disabled,
  label,
  onChange,
  value,
}: {
  disabled: boolean;
  label: string;
  onChange: (value: number) => void;
  value: number;
}) {
  return (
    <label className="field-block">
      <span>{label}</span>
      <input
        className="number-input"
        disabled={disabled}
        min={1}
        onChange={(event) => onChange(Number(event.target.value) || 1)}
        type="number"
        value={value}
      />
    </label>
  );
}

function PresetStrip({
  disabled,
  items,
  onSelect,
  prefix,
  selected,
  suffix,
}: {
  disabled: boolean;
  items: number[];
  onSelect: (value: number) => void;
  prefix: string;
  selected: number;
  suffix: string;
}) {
  return (
    <div className="preset-strip">
      {items.map((value) => (
        <button
          className={selected === value ? 'chip active' : 'chip'}
          disabled={disabled}
          key={`${prefix}-${value}`}
          onClick={() => onSelect(value)}
          type="button"
        >
          {prefix}{value}{suffix}
        </button>
      ))}
    </div>
  );
}

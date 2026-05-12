import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  BellRing,
  BookOpen,
  CheckCircle2,
  Coffee,
  Gauge,
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
  startStudyMode,
} from '../services/focusApi';
import { notifyStudyReminder } from '../services/alertApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { getAppSettings } from '../services/settingsApi';
import type { FocusMode, FocusSession, FocusStatsSummary, StudyModePhase, StudyModeState, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';

const studyPresetMinutes = [60, 120, 180, 240];
const focusPresetMinutes = [25, 45, 60, 90];
const breakPresetMinutes = [5, 10, 15, 20];

const idleStudyState: StudyModeState = {
  id: null,
  phase: 'idle',
  status: 'idle',
  mode: 'normal',
  subject_id: null,
  planned_seconds: 0,
  focus_seconds: 0,
  break_seconds: 0,
  cycle_index: 0,
  started_at: null,
  phase_started_at: null,
  ended_at: null,
  current_session: null,
  study_elapsed_seconds: 0,
  study_remaining_seconds: 0,
  phase_elapsed_seconds: 0,
  phase_remaining_seconds: 0,
  focus_enforcement_active: false,
};

const phaseLabel: Record<StudyModePhase, string> = {
  idle: '待开始',
  focus: '专注中',
  awaiting_break: '等待确认休息',
  break: '休息中',
  finished: '已完成',
  emergency_exited: '已退出（历史）',
};

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
  const lastReminderKeyRef = useRef<string | null>(null);
  const initializedReminderRef = useRef(false);

  const active = studyState.status === 'active';
  const controlsDisabled = active;
  const currentSession = studyState.current_session;
  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);
  const displayedSubjectId = active ? studyState.subject_id : selectedSubjectId;
  const selectedSubjectName = displayedSubjectId ? subjectNameMap.get(displayedSubjectId) : null;
  const timerValue = studyState.phase === 'idle'
    ? formatSeconds(focusMinutes * 60)
    : studyState.phase === 'awaiting_break'
      ? '00:00'
      : formatSeconds(studyState.phase_remaining_seconds);
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
    if (!initializedReminderRef.current) {
      initializedReminderRef.current = true;
      lastReminderKeyRef.current = reminderKey(studyState);
      return;
    }

    const key = reminderKey(studyState);
    if (key === lastReminderKeyRef.current) {
      return;
    }

    lastReminderKeyRef.current = key;
    const reminder = buildReminder(studyState);
    if (reminder) {
      void notifyStudyReminder(reminder);
    }
  }, [studyState.id, studyState.phase, studyState.status, studyState.cycle_index]);

  async function initializePage() {
    try {
      setError(null);
      const [settings, state] = await Promise.all([getAppSettings(), getStudyModeState()]);
      setStudyState(state);

      if (state.status !== 'active') {
        setStudyMinutes(settings.default_study_minutes);
        setFocusMinutes(settings.default_focus_minutes);
        setBreakMinutes(settings.break_minutes);
        setMode(settings.default_focus_mode);
      } else {
        setStudyMinutes(Math.max(1, Math.round(state.planned_seconds / 60)));
        setFocusMinutes(Math.max(1, Math.round(state.focus_seconds / 60)));
        setBreakMinutes(Math.max(1, Math.round(state.break_seconds / 60)));
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
        mode,
        selectedSubjectId,
      );
      setStudyState(nextState);
      setNotice('学习模式已开始。关闭窗口会进入托盘，后台仍会计时并执行白名单。');
      lastReminderKeyRef.current = reminderKey(nextState);
      void notifyStudyReminder({
        title: '学习模式已开始',
        body: `第 ${nextState.cycle_index} 轮番茄钟开始，保持专注。`,
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
      setNotice('休息已开始。休息结束后会自动进入下一轮番茄钟。');
      lastReminderKeyRef.current = reminderKey(nextState);
      void notifyStudyReminder({
        title: '休息开始',
        body: `休息 ${formatDuration(nextState.break_seconds)}，到点后自动进入下一轮番茄钟。`,
      });
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="page-shell focus-workbench">
      <header className="page-header">
        <div>
          <p className="eyebrow">学习模式 / 后台番茄钟</p>
          <h2>专注控制台</h2>
          <p>启动后窗口可隐藏到托盘，后台继续计时、切换阶段并执行白名单。</p>
        </div>
        <div className={`phase-badge phase-${studyState.phase}`}>
          <span>{phaseLabel[studyState.phase]}</span>
          <strong>{active ? `第 ${studyState.cycle_index} 轮` : '后台待命'}</strong>
        </div>
      </header>

      {error && <p className="alert error">{error}</p>}
      {notice && <p className="alert success">{notice}</p>}
      {monitorError && <p className="alert error">前台检测失败：{monitorError}</p>}

      <div className="focus-grid">
        <section className="timer-console">
          <div className="timer-topline">
            <span>{studyState.phase === 'awaiting_break' ? '等待本人确认' : '当前阶段倒计时'}</span>
            <span>{active ? `开始于 ${formatDateTime(studyState.started_at)}` : '准备开始'}</span>
          </div>
          <div className="timer-display">{timerValue}</div>
          <p className="timer-caption">{buildPhaseMessage(studyState)}</p>

          <div className="progress-stack">
            <ProgressBar label="学习总进度" value={totalProgress} />
            <ProgressBar label="当前阶段进度" value={phaseProgress} accent />
          </div>

          <div className="metric-strip">
            <Metric icon={Timer} label="总剩余" value={formatSeconds(active ? studyState.study_remaining_seconds : studyMinutes * 60)} />
            <Metric icon={Activity} label="阶段" value={phaseLabel[studyState.phase]} />
            <Metric
              icon={ShieldCheck}
              label="强制执行"
              value={studyState.focus_enforcement_active ? '运行中' : active ? '休息暂停' : '未启动'}
            />
          </div>

          <div className="action-group">
            {!active ? (
              <button className="primary-action" onClick={handleStart} type="button">
                <Play size={18} />
                开始学习模式
              </button>
            ) : studyState.phase === 'awaiting_break' ? (
              <button className="primary-action" onClick={handleConfirmBreak} type="button">
                <Coffee size={18} />
                确认开始休息
              </button>
            ) : (
              <button className="secondary-action" onClick={refreshStudyState} type="button">
                <RefreshCw size={18} />
                刷新状态
              </button>
            )}
          </div>
        </section>

        <aside className="side-stack">
          <section className="panel">
            <div className="panel-title">
              <div>
                <p className="eyebrow">Plan</p>
                <h3>学习计划</h3>
              </div>
              <BookOpen size={20} />
            </div>

            <div className="field-grid">
              <NumberField disabled={controlsDisabled} label="学习模式" onChange={setStudyMinutes} value={studyMinutes} />
              <NumberField disabled={controlsDisabled} label="番茄钟" onChange={setFocusMinutes} value={focusMinutes} />
              <NumberField disabled={controlsDisabled} label="休息" onChange={setBreakMinutes} value={breakMinutes} />
            </div>

            <PresetStrip disabled={controlsDisabled} items={studyPresetMinutes} prefix="" selected={studyMinutes} suffix="m" onSelect={setStudyMinutes} />
            <PresetStrip disabled={controlsDisabled} items={focusPresetMinutes} prefix="专注 " selected={focusMinutes} suffix="m" onSelect={setFocusMinutes} />
            <PresetStrip disabled={controlsDisabled} items={breakPresetMinutes} prefix="休息 " selected={breakMinutes} suffix="m" onSelect={setBreakMinutes} />

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
          </section>

          <section className="panel monitor-panel">
            <div className="panel-title">
              <div>
                <p className="eyebrow">Monitor</p>
                <h3>前台监控</h3>
              </div>
              <Gauge size={20} />
            </div>
            {latestAppCheck ? (
              <div className={latestAppCheck.match_result.allowed ? 'monitor-card allowed' : 'monitor-card blocked'}>
                <div>
                  <span>{latestAppCheck.match_result.allowed ? '已放行' : '已拦截'}</span>
                  <strong>{latestAppCheck.foreground_app.process_name}</strong>
                  <p>{latestAppCheck.foreground_app.window_title || '无窗口标题'}</p>
                  {latestAppCheck.match_result.detected_domain && <p>识别网站：{latestAppCheck.match_result.detected_domain}</p>}
                </div>
                <div className="monitor-count">
                  <span>累计拦截</span>
                  <strong>{latestAppCheck.interruption_count}</strong>
                </div>
              </div>
            ) : (
              <div className="empty-state compact">
                学习阶段会自动检查前台窗口。关闭主界面到托盘后，后端仍会继续执行。
              </div>
            )}
          </section>
        </aside>
      </div>

      <div className="overview-grid">
        <section className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Today</p>
              <h3>今日统计</h3>
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

        <section className="panel">
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
              {history.slice(0, 5).map((session) => (
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
    return studyState.break_seconds;
  }

  return 0;
}

function buildPhaseMessage(studyState: StudyModeState) {
  if (studyState.phase === 'focus') {
    return `第 ${studyState.cycle_index} 轮番茄钟进行中。窗口关到托盘后，后台仍会计时并关闭非白名单窗口。`;
  }

  if (studyState.phase === 'awaiting_break') {
    return '本轮番茄钟已到点。确认休息前，学习时间继续累计，白名单强制执行继续生效。';
  }

  if (studyState.phase === 'break') {
    return '正在休息。休息时间结束后，后台会自动进入下一轮番茄钟。';
  }

  if (studyState.phase === 'finished') {
    return '学习模式已完成，记录已写入本地 SQLite 数据库。';
  }

  if (studyState.phase === 'emergency_exited') {
    return '检测到旧版本留下的退出状态。当前版本不再提供提前退出入口。';
  }

  return '设置学习时长、番茄钟和休息时间后开始。';
}

function reminderKey(studyState: StudyModeState) {
  return [
    studyState.id ?? 'idle',
    studyState.status,
    studyState.phase,
    studyState.cycle_index,
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
      body: '本轮已经到点。请确认开始休息；未确认前学习时间会继续累计。',
    };
  }

  if (studyState.status === 'active' && studyState.phase === 'break') {
    return {
      title: '休息开始',
      body: `休息 ${formatDuration(studyState.break_seconds)}，结束后自动进入下一轮番茄钟。`,
    };
  }

  if (studyState.status === 'finished' || studyState.phase === 'finished') {
    return {
      title: '学习模式完成',
      body: `本次学习已完成，共累计 ${formatDuration(studyState.study_elapsed_seconds)}。`,
    };
  }

  if (studyState.status === 'emergency_exited' || studyState.phase === 'emergency_exited') {
    return {
      title: '学习模式已退出',
      body: '这是旧版本兼容状态。当前版本学习模式只能自然完成。',
    };
  }

  return null;
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
      {items.map((minutes) => (
        <button
          className={selected === minutes ? 'chip active' : 'chip'}
          disabled={disabled}
          key={`${prefix}-${minutes}`}
          onClick={() => onSelect(minutes)}
          type="button"
        >
          {prefix}{minutes}{suffix}
        </button>
      ))}
    </div>
  );
}

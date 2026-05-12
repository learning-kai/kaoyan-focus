import { useEffect, useMemo, useState } from 'react';
import {
  confirmStudyBreak,
  emergencyExitStudyMode,
  getFocusStatsSummary,
  getStudyModeState,
  listFocusSessions,
  listSubjects,
  resetStudyMode,
  startStudyMode,
} from '../services/focusApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { getAppSettings } from '../services/settingsApi';
import type { FocusMode, FocusSession, FocusStatsSummary, StudyModePhase, StudyModeState, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';

const studyPresetMinutes = [60, 120, 180, 240];
const focusPresetMinutes = [25, 45, 60, 90];
const breakPresetMinutes = [5, 10, 15, 20];
const emergencyConfirmText = '我确认应急退出';

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
  emergency_exited: '应急退出',
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
    emergency_exited: '应急退出',
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
  const [emergencyExitOpen, setEmergencyExitOpen] = useState(false);
  const [emergencyCooldownSeconds, setEmergencyCooldownSeconds] = useState(60);
  const [emergencyCooldownRemaining, setEmergencyCooldownRemaining] = useState(60);
  const [emergencyConfirmValue, setEmergencyConfirmValue] = useState('');

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
    if (!emergencyExitOpen) {
      return;
    }

    setEmergencyCooldownRemaining(emergencyCooldownSeconds);
    const intervalId = window.setInterval(() => {
      setEmergencyCooldownRemaining((current) => Math.max(current - 1, 0));
    }, 1000);

    return () => window.clearInterval(intervalId);
  }, [emergencyCooldownSeconds, emergencyExitOpen]);

  async function initializePage() {
    try {
      setError(null);
      const [settings, state] = await Promise.all([getAppSettings(), getStudyModeState()]);
      setEmergencyCooldownSeconds(settings.emergency_cooldown_seconds);
      setEmergencyCooldownRemaining(settings.emergency_cooldown_seconds);
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
        closeEmergencyExitPanel();
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
      closeEmergencyExitPanel();
      const nextState = await startStudyMode(
        studyMinutes * 60,
        focusMinutes * 60,
        breakMinutes * 60,
        mode,
        selectedSubjectId,
      );
      setStudyState(nextState);
      setNotice('学习模式已开始。关闭窗口会进入托盘，后台仍会计时和执行白名单。');
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
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleEmergencyExit() {
    try {
      setError(null);
      const nextState = await emergencyExitStudyMode();
      setStudyState(nextState);
      setLatestAppCheck(null);
      setMonitorError(null);
      setNotice('已执行应急退出，本次严格模式退出次数已记录。');
      closeEmergencyExitPanel();
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleReset() {
    try {
      setError(null);
      const nextState = await resetStudyMode();
      setStudyState(nextState);
      setLatestAppCheck(null);
      setMonitorError(null);
      setNotice('学习模式已结束，后台强制执行已停止。');
      closeEmergencyExitPanel();
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  function openEmergencyExitPanel() {
    setEmergencyConfirmValue('');
    setEmergencyExitOpen(true);
  }

  function closeEmergencyExitPanel() {
    setEmergencyExitOpen(false);
    setEmergencyConfirmValue('');
    setEmergencyCooldownRemaining(emergencyCooldownSeconds);
  }

  return (
    <section className="focus-workbench">
      <header className="workbench-header">
        <div>
          <p className="eyebrow">学习模式 / 后台番茄钟</p>
          <h2>考研专注控制台</h2>
        </div>
        <div className={`phase-badge phase-${studyState.phase}`}>
          <span>{phaseLabel[studyState.phase]}</span>
          <strong>{active ? `第 ${studyState.cycle_index} 轮` : '后台待命'}</strong>
        </div>
      </header>

      {error && <p className="error-text">{error}</p>}
      {notice && <p className="success-text">{notice}</p>}
      {monitorError && <p className="error-text">前台检测失败：{monitorError}</p>}

      <div className="status-ribbon">
        <article>
          <span>学习总剩余</span>
          <strong>{formatSeconds(active ? studyState.study_remaining_seconds : studyMinutes * 60)}</strong>
        </article>
        <article>
          <span>本轮阶段</span>
          <strong>{phaseLabel[studyState.phase]}</strong>
        </article>
        <article>
          <span>强制执行</span>
          <strong>{studyState.focus_enforcement_active ? '运行中' : active ? '休息暂停' : '未启动'}</strong>
        </article>
        <article>
          <span>科目</span>
          <strong>{selectedSubjectName ?? '未指定'}</strong>
        </article>
      </div>

      <div className="workbench-grid">
        <section className="timer-console">
          <div className="timer-topline">
            <span>{studyState.phase === 'awaiting_break' ? '等待本人确认' : '当前倒计时'}</span>
            <span>{active ? formatDateTime(studyState.started_at) : '准备开始'}</span>
          </div>
          <div className="timer-display compact">{timerValue}</div>
          <p className="timer-caption">{buildPhaseMessage(studyState)}</p>

          <div className="progress-stack">
            <div>
              <span>学习进度</span>
              <div className="progress-track"><i style={{ width: `${totalProgress}%` }} /></div>
            </div>
            <div>
              <span>阶段进度</span>
              <div className="progress-track accent"><i style={{ width: `${phaseProgress}%` }} /></div>
            </div>
          </div>

          <div className="action-group console-actions">
            {!active ? (
              <button className="primary-action" onClick={handleStart} type="button">
                开始学习模式
              </button>
            ) : studyState.phase === 'awaiting_break' ? (
              <button className="primary-action" onClick={handleConfirmBreak} type="button">
                确认开始休息
              </button>
            ) : (
              <button className="secondary-action" onClick={refreshStudyState} type="button">
                刷新状态
              </button>
            )}

            {active && (
              <button className="secondary-action danger-outline" onClick={handleReset} type="button">
                结束学习模式
              </button>
            )}

            {active && studyState.mode === 'strict' && (
              <button className="secondary-action danger-outline" onClick={openEmergencyExitPanel} type="button">
                应急退出
              </button>
            )}
          </div>

          {emergencyExitOpen && active && studyState.mode === 'strict' && (
            <div className="emergency-panel">
              <div>
                <strong>严格模式应急退出</strong>
                <p>等待冷却结束并输入确认文本后，才会结束当前学习模式。</p>
              </div>
              <div className="emergency-countdown">
                <span>冷却剩余</span>
                <strong>{formatSeconds(emergencyCooldownRemaining)}</strong>
              </div>
              <label className="field-label" htmlFor="emergency-confirm">确认文本</label>
              <input
                className="text-input"
                id="emergency-confirm"
                onChange={(event) => setEmergencyConfirmValue(event.target.value)}
                placeholder={emergencyConfirmText}
                value={emergencyConfirmValue}
              />
              <div className="emergency-actions">
                <button className="secondary-action" onClick={closeEmergencyExitPanel} type="button">取消</button>
                <button
                  className="danger-solid"
                  disabled={emergencyCooldownRemaining > 0 || emergencyConfirmValue.trim() !== emergencyConfirmText}
                  onClick={handleEmergencyExit}
                  type="button"
                >
                  确认退出
                </button>
              </div>
            </div>
          )}
        </section>

        <aside className="side-stack">
          <section className="control-panel">
            <h3>学习计划</h3>
            <div className="field-grid">
              <label>
                <span>学习模式时长</span>
                <input
                  className="number-input"
                  disabled={controlsDisabled}
                  min={1}
                  onChange={(event) => setStudyMinutes(Number(event.target.value) || 1)}
                  type="number"
                  value={studyMinutes}
                />
              </label>
              <label>
                <span>番茄钟时长</span>
                <input
                  className="number-input"
                  disabled={controlsDisabled}
                  min={1}
                  onChange={(event) => setFocusMinutes(Number(event.target.value) || 1)}
                  type="number"
                  value={focusMinutes}
                />
              </label>
              <label>
                <span>休息时长</span>
                <input
                  className="number-input"
                  disabled={controlsDisabled}
                  min={1}
                  onChange={(event) => setBreakMinutes(Number(event.target.value) || 1)}
                  type="number"
                  value={breakMinutes}
                />
              </label>
            </div>

            <div className="preset-strip">
              {studyPresetMinutes.map((minutes) => (
                <button
                  className={studyMinutes === minutes ? 'chip active' : 'chip'}
                  disabled={controlsDisabled}
                  key={minutes}
                  onClick={() => setStudyMinutes(minutes)}
                  type="button"
                >
                  {minutes}m
                </button>
              ))}
            </div>

            <div className="preset-strip">
              {focusPresetMinutes.map((minutes) => (
                <button
                  className={focusMinutes === minutes ? 'chip active' : 'chip'}
                  disabled={controlsDisabled}
                  key={minutes}
                  onClick={() => setFocusMinutes(minutes)}
                  type="button"
                >
                  专注 {minutes}m
                </button>
              ))}
            </div>

            <div className="preset-strip">
              {breakPresetMinutes.map((minutes) => (
                <button
                  className={breakMinutes === minutes ? 'chip active' : 'chip'}
                  disabled={controlsDisabled}
                  key={minutes}
                  onClick={() => setBreakMinutes(minutes)}
                  type="button"
                >
                  休息 {minutes}m
                </button>
              ))}
            </div>

            <label className="subject-picker">
              <span className="field-label">科目</span>
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

            <div className="mode-switch compact-mode">
              <button className={mode === 'normal' ? 'mode active' : 'mode'} disabled={controlsDisabled} onClick={() => setMode('normal')} type="button">
                普通模式
              </button>
              <button className={mode === 'strict' ? 'mode active' : 'mode'} disabled={controlsDisabled} onClick={() => setMode('strict')} type="button">
                严格模式
              </button>
            </div>
          </section>

          <section className="monitor-compact">
            <div className="panel-heading-row">
              <h3>前台监控</h3>
              <span>{studyState.focus_enforcement_active ? '后台执行中' : '待命'}</span>
            </div>
            {latestAppCheck ? (
              <div className={latestAppCheck.match_result.allowed ? 'monitor-card allowed slim' : 'monitor-card blocked slim'}>
                <div>
                  <span>{latestAppCheck.match_result.reason}</span>
                  <strong>{latestAppCheck.foreground_app.process_name}</strong>
                  <p>{latestAppCheck.foreground_app.window_title || '无窗口标题'}</p>
                  {latestAppCheck.match_result.detected_domain && <p>识别网站：{latestAppCheck.match_result.detected_domain}</p>}
                </div>
                <div>
                  <span>累计拦截</span>
                  <strong>{latestAppCheck.interruption_count}</strong>
                </div>
              </div>
            ) : (
              <div className="empty-state compact-empty">学习阶段会自动检查前台窗口。关闭主界面到托盘后，后端仍会继续执行。</div>
            )}
          </section>
        </aside>
      </div>

      <div className="overview-grid">
        <section className="mini-panel">
          <h3>今日统计</h3>
          <div className="stats-grid focus-stats">
            <article className="stat-card">
              <span>今日</span>
              <strong>{formatDuration(stats?.today_seconds ?? 0)}</strong>
            </article>
            <article className="stat-card">
              <span>本周</span>
              <strong>{formatDuration(stats?.week_seconds ?? 0)}</strong>
            </article>
            <article className="stat-card">
              <span>本月</span>
              <strong>{formatDuration(stats?.month_seconds ?? 0)}</strong>
            </article>
            <article className="stat-card">
              <span>拦截</span>
              <strong>{stats?.interruption_count ?? 0}</strong>
            </article>
          </div>
        </section>

        <section className="mini-panel">
          <h3>最近记录</h3>
          {history.length === 0 ? (
            <div className="empty-state compact-empty">还没有专注记录。</div>
          ) : (
            <div className="compact-history">
              {history.slice(0, 5).map((session) => (
                <article className="history-row compact-row" key={session.id}>
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
    return '严格模式已应急退出，本次退出已记录。';
  }

  return '设置学习时长、番茄钟和休息时间后开始。';
}

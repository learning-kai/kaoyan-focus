import { useEffect, useMemo, useRef, useState } from 'react';
import { emergencyExitFocusSession, finishFocusSession, getFocusStatsSummary, interruptFocusSession, listFocusSessions, listSubjects, recoverActiveFocusSession, setStudyModeActive, startFocusSession } from '../services/focusApi';
import { checkFocusForegroundApp } from '../services/monitorApi';
import { getAppSettings } from '../services/settingsApi';
import type { FocusMode, FocusSession, FocusSessionRecovery, FocusStatsSummary, FocusStatus, Subject } from '../types/focus';
import type { FocusAppCheck } from '../types/monitor';

type StudyPhase = 'idle' | 'focus' | 'awaiting_break' | 'break' | 'finished' | 'interrupted' | 'emergency_exited';

const studyPresetMinutes = [30, 60, 120, 180];
const focusPresetMinutes = [1, 25, 45, 60];
const emergencyConfirmText = '我确认应急退出';

function formatSeconds(totalSeconds: number) {
  const minutes = Math.floor(totalSeconds / 60).toString().padStart(2, '0');
  const seconds = Math.max(0, totalSeconds % 60).toString().padStart(2, '0');
  return `${minutes}:${seconds}`;
}

function formatDuration(seconds: number) {
  if (seconds < 60) {
    return `${seconds} 秒`;
  }

  return `${Math.round(seconds / 60)} 分钟`;
}

function formatStudyTime(seconds: number) {
  if (seconds < 3600) {
    return `${Math.round(seconds / 60)} 分钟`;
  }

  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

export default function FocusPage() {
  const [studyMinutes, setStudyMinutes] = useState(120);
  const [focusMinutes, setFocusMinutes] = useState(25);
  const [breakMinutes, setBreakMinutes] = useState(5);
  const [mode, setMode] = useState<FocusMode>('normal');
  const [status, setStatus] = useState<FocusStatus>('idle');
  const [studyPhase, setStudyPhase] = useState<StudyPhase>('idle');
  const [activeSession, setActiveSession] = useState<FocusSession | null>(null);
  const [remainingSeconds, setRemainingSeconds] = useState(focusMinutes * 60);
  const [studyRemainingSeconds, setStudyRemainingSeconds] = useState(studyMinutes * 60);
  const [cycleIndex, setCycleIndex] = useState(0);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [studyStartedAt, setStudyStartedAt] = useState<number | null>(null);
  const [breakStartedAt, setBreakStartedAt] = useState<number | null>(null);
  const [history, setHistory] = useState<FocusSession[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [selectedSubjectId, setSelectedSubjectId] = useState<number | null>(null);
  const [stats, setStats] = useState<FocusStatsSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [latestAppCheck, setLatestAppCheck] = useState<FocusAppCheck | null>(null);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [recoveryMessage, setRecoveryMessage] = useState<string | null>(null);
  const [pendingRecovery, setPendingRecovery] = useState<FocusSessionRecovery | null>(null);
  const [emergencyExitOpen, setEmergencyExitOpen] = useState(false);
  const [emergencyCooldownSeconds, setEmergencyCooldownSeconds] = useState(60);
  const [emergencyCooldownRemaining, setEmergencyCooldownRemaining] = useState(60);
  const [emergencyConfirmValue, setEmergencyConfirmValue] = useState('');
  const finishingSessionRef = useRef(false);
  const startingNextSessionRef = useRef(false);

  const studyPlannedSeconds = studyMinutes * 60;
  const focusPlannedSeconds = focusMinutes * 60;
  const breakPlannedSeconds = breakMinutes * 60;
  const focusEnforcementActive = studyPhase === 'focus' || studyPhase === 'awaiting_break';
  const controlsDisabled = studyPhase !== 'idle' || pendingRecovery !== null;

  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);

  useEffect(() => {
    if (studyPhase === 'idle') {
      setRemainingSeconds(focusPlannedSeconds);
      setStudyRemainingSeconds(studyPlannedSeconds);
    }
  }, [focusPlannedSeconds, studyPhase, studyPlannedSeconds]);

  useEffect(() => {
    void initializeFocusPage();
  }, []);

  useEffect(() => {
    if ((studyPhase !== 'focus' && studyPhase !== 'awaiting_break') || studyStartedAt === null) {
      return;
    }

    const tick = async () => {
      const studyElapsedSeconds = Math.floor((Date.now() - studyStartedAt) / 1000);
      const nextStudyRemainingSeconds = Math.max(studyPlannedSeconds - studyElapsedSeconds, 0);
      setStudyRemainingSeconds(nextStudyRemainingSeconds);

      if (nextStudyRemainingSeconds === 0) {
        await completeStudyMode();
        return;
      }

      if (studyPhase === 'focus' && startedAt !== null && activeSession !== null) {
        const focusElapsedSeconds = Math.floor((Date.now() - startedAt) / 1000);
        const nextFocusRemainingSeconds = Math.max(activeSession.planned_seconds - focusElapsedSeconds, 0);
        setRemainingSeconds(nextFocusRemainingSeconds);

        if (nextFocusRemainingSeconds === 0) {
          await finishCurrentPomodoro();
        }
      }
    };

    void tick();
    const intervalId = window.setInterval(() => void tick(), 1000);
    return () => window.clearInterval(intervalId);
  }, [activeSession, startedAt, studyPhase, studyPlannedSeconds, studyStartedAt]);

  useEffect(() => {
    if (!focusEnforcementActive || activeSession === null) {
      return;
    }

    const checkForegroundApp = async () => {
      try {
        setMonitorError(null);
        const result = await checkFocusForegroundApp(activeSession.id);
        setLatestAppCheck(result);
      } catch (reason) {
        setMonitorError(reason instanceof Error ? reason.message : String(reason));
      }
    };

    void checkForegroundApp();
    const intervalId = window.setInterval(() => void checkForegroundApp(), 3000);
    return () => window.clearInterval(intervalId);
  }, [activeSession, focusEnforcementActive]);

  useEffect(() => {
    if (studyPhase !== 'break' || breakStartedAt === null) {
      return;
    }

    const tickBreak = async () => {
      if (studyStartedAt !== null) {
        const studyElapsedSeconds = Math.floor((Date.now() - studyStartedAt) / 1000);
        const nextStudyRemainingSeconds = Math.max(studyPlannedSeconds - studyElapsedSeconds, 0);
        setStudyRemainingSeconds(nextStudyRemainingSeconds);

        if (nextStudyRemainingSeconds === 0) {
          await completeStudyMode();
          return;
        }
      }

      const breakElapsedSeconds = Math.floor((Date.now() - breakStartedAt) / 1000);
      const nextBreakRemainingSeconds = Math.max(breakPlannedSeconds - breakElapsedSeconds, 0);
      setRemainingSeconds(nextBreakRemainingSeconds);

      if (nextBreakRemainingSeconds === 0) {
        await startNextPomodoro();
      }
    };

    void tickBreak();
    const intervalId = window.setInterval(() => void tickBreak(), 1000);
    return () => window.clearInterval(intervalId);
  }, [breakPlannedSeconds, breakStartedAt, studyPhase, studyPlannedSeconds, studyStartedAt]);

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

  const summary = useMemo(() => {
    if (studyPhase === 'focus' || studyPhase === 'awaiting_break') {
      const subjectName = activeSession?.subject_id ? subjectNameMap.get(activeSession.subject_id) : null;
      const base = subjectName
        ? `学习模式进行中：正在累计 ${subjectName} 学习时长，并持续检测非白名单前台应用，发现后会尝试关闭。`
        : '学习模式进行中：正在累计学习时长，并持续检测非白名单前台应用，发现后会尝试关闭。';
      if (studyPhase === 'awaiting_break') {
        return `${base} 本轮番茄钟已到点，确认开始休息前仍会继续累计学习时间并执行白名单。`;
      }
      return activeSession?.mode === 'strict' ? `${base} 严格模式下可使用应急退出提前结束，并会记录次数。` : base;
    }

    if (studyPhase === 'break') {
      return '休息中：休息倒计时结束后会自动进入下一个番茄钟。';
    }

    if (studyPhase === 'finished') {
      return '本次学习模式已完成，记录和统计汇总已经写入本地数据库。';
    }

    if (studyPhase === 'emergency_exited') {
      return '本次严格模式专注已应急退出，退出次数已经写入本地数据库。';
    }

    if (studyPhase === 'interrupted') {
      return '上次专注已经按异常中断记录，可从最近专注记录里查看详情。';
    }

    if (pendingRecovery) {
      return `检测到上次未完成的专注，剩余 ${formatDuration(pendingRecovery.remaining_seconds)}。你可以继续，也可以标记为中断。`;
    }

    return mode === 'normal'
      ? '学习模式：在设定总时长内强制执行番茄钟，休息必须由本人确认开始。'
      : '严格模式：在学习模式内强制执行番茄钟，并支持应急退出记录。';
  }, [activeSession?.mode, activeSession?.subject_id, mode, pendingRecovery, studyPhase, subjectNameMap]);

  async function refreshDashboard() {
    try {
      setError(null);
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

  async function initializeFocusPage() {
    try {
      setError(null);
      const [settings, recovery] = await Promise.all([getAppSettings(), recoverActiveFocusSession()]);
      setEmergencyCooldownSeconds(settings.emergency_cooldown_seconds);
      setEmergencyCooldownRemaining(settings.emergency_cooldown_seconds);
      await refreshDashboard();

      if (recovery === null) {
        setStudyMinutes(settings.default_study_minutes);
        setFocusMinutes(settings.default_focus_minutes);
        setBreakMinutes(settings.break_minutes);
        setRemainingSeconds(settings.default_focus_minutes * 60);
        setStudyRemainingSeconds(settings.default_study_minutes * 60);
        setMode(settings.default_focus_mode);
        return;
      }

      if (recovery.recovery_status === 'interrupted_after_due') {
        setActiveSession(recovery.session);
        setRemainingSeconds(0);
        setStudyRemainingSeconds(0);
        setStartedAt(null);
        setStudyStartedAt(null);
        setBreakStartedAt(null);
        setStatus('interrupted');
        setStudyPhase('interrupted');
        setRecoveryMessage('检测到上次专注已超过计划时间，已按异常中断记录。');
        await refreshDashboard();
        return;
      }

      const recoveredMinutes = Math.max(1, Math.ceil(recovery.session.planned_seconds / 60));
      setFocusMinutes(recoveredMinutes);
      setStudyMinutes(recoveredMinutes);
      setBreakMinutes(settings.break_minutes);
      setMode(recovery.session.mode);
      setSelectedSubjectId(recovery.session.subject_id);
      setActiveSession(recovery.session);
      setRemainingSeconds(recovery.remaining_seconds);
      setStudyRemainingSeconds(recovery.remaining_seconds);
      setStartedAt(null);
      setStudyStartedAt(null);
      setBreakStartedAt(null);
      setPendingRecovery(recovery);
      setRecoveryMessage('检测到上次未完成的专注，请选择继续或标记中断。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleStart() {
    try {
      setError(null);
      setMonitorError(null);
      setLatestAppCheck(null);
      setRecoveryMessage(null);
      setPendingRecovery(null);
      closeEmergencyExitPanel();
      const now = Date.now();
      await setStudyModeActive(true);
      const session = await startFocusSession(focusPlannedSeconds, mode, selectedSubjectId);
      setActiveSession(session);
      setRemainingSeconds(focusPlannedSeconds);
      setStudyRemainingSeconds(studyPlannedSeconds);
      setCycleIndex(1);
      setStartedAt(now);
      setStudyStartedAt(now);
      setBreakStartedAt(null);
      setStatus('running');
      setStudyPhase('focus');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  function handleResumeRecoveredSession() {
    if (!pendingRecovery) {
      return;
    }

    setActiveSession(pendingRecovery.session);
    setRemainingSeconds(pendingRecovery.remaining_seconds);
    setStudyRemainingSeconds(pendingRecovery.remaining_seconds);
    const now = Date.now();
    setStartedAt(now - pendingRecovery.elapsed_seconds * 1000);
    setStudyStartedAt(now - pendingRecovery.elapsed_seconds * 1000);
    setBreakStartedAt(null);
    setCycleIndex(1);
    setStatus('running');
    setStudyPhase('focus');
    void setStudyModeActive(true);
    setRecoveryMessage(`已恢复上次未完成的专注，剩余 ${formatDuration(pendingRecovery.remaining_seconds)}。`);
    setPendingRecovery(null);
  }

  async function handleInterruptRecoveredSession() {
    if (!pendingRecovery) {
      return;
    }

    try {
      setError(null);
      const session = await interruptFocusSession(pendingRecovery.session.id, pendingRecovery.elapsed_seconds);
      await setStudyModeActive(false);
      setActiveSession(session);
      setStartedAt(null);
      setStudyStartedAt(null);
      setBreakStartedAt(null);
      setRemainingSeconds(0);
      setStudyRemainingSeconds(0);
      setStatus('interrupted');
      setStudyPhase('interrupted');
      setRecoveryMessage('已将上次未完成专注标记为异常中断。');
      setPendingRecovery(null);
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleEmergencyExit() {
    if (activeSession === null || startedAt === null) {
      return;
    }

    try {
      setError(null);
      const actualSeconds = Math.max(Math.floor((Date.now() - startedAt) / 1000), 0);
      const session = await emergencyExitFocusSession(activeSession.id, actualSeconds);
      await setStudyModeActive(false);
      setActiveSession(session);
      setStartedAt(null);
      setStudyStartedAt(null);
      setBreakStartedAt(null);
      setStatus('emergency_exited');
      setStudyPhase('emergency_exited');
      setLatestAppCheck(null);
      setMonitorError(null);
      setRecoveryMessage(null);
      closeEmergencyExitPanel();
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function finishCurrentPomodoro() {
    if (activeSession === null || startedAt === null || finishingSessionRef.current) {
      return;
    }

    finishingSessionRef.current = true;
    try {
      setRemainingSeconds(0);
      setStudyPhase('awaiting_break');
      setRecoveryMessage('本轮番茄钟已到点。确认开始休息前，学习时间会继续累计，并继续执行白名单拦截。');
    } finally {
      finishingSessionRef.current = false;
    }
  }

  async function handleStartBreak() {
    if (activeSession === null || startedAt === null) {
      return;
    }

    try {
      setError(null);
      setMonitorError(null);
      setLatestAppCheck(null);
      const actualSeconds = Math.max(Math.floor((Date.now() - startedAt) / 1000), activeSession.planned_seconds);
      const finishedSession = await finishFocusSession(activeSession.id, actualSeconds);
      setActiveSession(finishedSession);
      setStartedAt(null);
      setBreakStartedAt(Date.now());
      setRemainingSeconds(breakPlannedSeconds);
      setStatus('finished');
      setStudyPhase('break');
      setRecoveryMessage(`第 ${cycleIndex} 个番茄钟已记录。休息结束后会自动进入下一轮。`);
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function startNextPomodoro() {
    if (startingNextSessionRef.current) {
      return;
    }

    startingNextSessionRef.current = true;
    try {
      setError(null);
      setMonitorError(null);
      setLatestAppCheck(null);
      if (studyRemainingSeconds <= 0) {
        await completeStudyMode();
        return;
      }

      const now = Date.now();
      const session = await startFocusSession(focusPlannedSeconds, mode, selectedSubjectId);
      setActiveSession(session);
      setStartedAt(now);
      setBreakStartedAt(null);
      setRemainingSeconds(focusPlannedSeconds);
      setCycleIndex((current) => current + 1);
      setStatus('running');
      setStudyPhase('focus');
      setRecoveryMessage('休息结束，已自动进入下一个番茄钟。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      startingNextSessionRef.current = false;
    }
  }

  async function completeStudyMode() {
    if (finishingSessionRef.current) {
      return;
    }

    finishingSessionRef.current = true;
    try {
      setMonitorError(null);
      setLatestAppCheck(null);
      await setStudyModeActive(false);

      if ((studyPhase === 'focus' || studyPhase === 'awaiting_break') && activeSession !== null && startedAt !== null) {
        const actualSeconds = Math.max(Math.floor((Date.now() - startedAt) / 1000), 0);
        const finishedSession = await finishFocusSession(activeSession.id, actualSeconds);
        setActiveSession(finishedSession);
      }

      setRemainingSeconds(0);
      setStudyRemainingSeconds(0);
      setStartedAt(null);
      setStudyStartedAt(null);
      setBreakStartedAt(null);
      setStatus('finished');
      setStudyPhase('finished');
      setRecoveryMessage('学习模式总时长已完成。');
      closeEmergencyExitPanel();
      await refreshDashboard();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      finishingSessionRef.current = false;
    }
  }

  async function handleReset() {
    setStatus('idle');
    setStudyPhase('idle');
    setActiveSession(null);
    setStartedAt(null);
    setStudyStartedAt(null);
    setBreakStartedAt(null);
    setRemainingSeconds(focusPlannedSeconds);
    setStudyRemainingSeconds(studyPlannedSeconds);
    setCycleIndex(0);
    setMonitorError(null);
    setLatestAppCheck(null);
    setRecoveryMessage(null);
    setPendingRecovery(null);
    closeEmergencyExitPanel();
    await setStudyModeActive(false);
    await refreshDashboard();
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
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">学习模式 / 番茄钟循环</p>
        <h2>学习模式</h2>
        <p>在设定学习时长内循环执行番茄钟；番茄到点后由本人确认休息，休息结束自动进入下一轮。</p>
      </div>

      <div className="focus-panel">
        <div className="timer-display">{formatSeconds(remainingSeconds)}</div>
        <p className="muted">
          {studyPhase === 'break'
            ? '休息倒计时'
            : studyPhase === 'awaiting_break'
              ? '等待确认休息'
              : studyPhase === 'focus'
                ? `第 ${cycleIndex} 个番茄钟`
                : '番茄专注时长'}
        </p>
      </div>

      <div className="stats-grid compact-stats">
        <article className="stat-card">
          <span>学习模式剩余</span>
          <strong>{formatSeconds(studyRemainingSeconds)}</strong>
        </article>
        <article className="stat-card">
          <span>番茄轮次</span>
          <strong>{cycleIndex || 0}</strong>
        </article>
      </div>

      <div className="subject-picker">
        <label className="field-label" htmlFor="study-minutes">学习模式时长</label>
        <input
          className="number-input"
          disabled={controlsDisabled}
          id="study-minutes"
          max={720}
          min={1}
          onChange={(event) => setStudyMinutes(Number(event.target.value) || 1)}
          type="number"
          value={studyMinutes}
        />
      </div>

      <div className="preset-grid">
        {studyPresetMinutes.map((value) => (
          <button
            className={value === studyMinutes ? 'chip active' : 'chip'}
            disabled={controlsDisabled}
            key={value}
            onClick={() => setStudyMinutes(value)}
            type="button"
          >
            {value} 分钟
          </button>
        ))}
      </div>

      <div className="pomodoro-grid">
        <div className="subject-picker">
          <label className="field-label" htmlFor="focus-minutes">番茄专注时长</label>
          <input
            className="number-input"
            disabled={controlsDisabled}
            id="focus-minutes"
            max={120}
            min={1}
            onChange={(event) => setFocusMinutes(Number(event.target.value) || 1)}
            type="number"
            value={focusMinutes}
          />
        </div>
        <div className="subject-picker">
          <label className="field-label" htmlFor="break-minutes">休息时长</label>
          <input
            className="number-input"
            disabled={controlsDisabled}
            id="break-minutes"
            max={60}
            min={1}
            onChange={(event) => setBreakMinutes(Number(event.target.value) || 1)}
            type="number"
            value={breakMinutes}
          />
        </div>
      </div>

      <div className="preset-grid">
        {focusPresetMinutes.map((value) => (
          <button
            className={value === focusMinutes ? 'chip active' : 'chip'}
            disabled={controlsDisabled}
            key={value}
            onClick={() => setFocusMinutes(value)}
            type="button"
          >
            番茄 {value} 分钟
          </button>
        ))}
      </div>

      <div className="mode-switch">
        <button className={mode === 'normal' ? 'mode active' : 'mode'} disabled={controlsDisabled} onClick={() => setMode('normal')} type="button">
          普通模式
        </button>
        <button className={mode === 'strict' ? 'mode active' : 'mode'} disabled={controlsDisabled} onClick={() => setMode('strict')} type="button">
          严格模式
        </button>
      </div>

      <div className="subject-picker">
        <label className="field-label" htmlFor="focus-subject">本次专注科目</label>
        <select
          className="select-input"
          disabled={controlsDisabled}
          id="focus-subject"
          onChange={(event) => setSelectedSubjectId(event.target.value ? Number(event.target.value) : null)}
          value={selectedSubjectId ?? ''}
        >
          <option value="">未分类</option>
          {subjects.map((subject) => (
            <option key={subject.id} value={subject.id}>
              {subject.name}
            </option>
          ))}
        </select>
      </div>

      <p className="notice">{summary}</p>
      {recoveryMessage && <p className={studyPhase === 'interrupted' ? 'warning-text' : 'success-text'}>{recoveryMessage}</p>}
      {error && <p className="error-text">{error}</p>}
      {monitorError && <p className="error-text">前台检测失败：{monitorError}</p>}

      {stats && (
        <div className="stats-grid compact-stats">
          <article className="stat-card">
            <span>今日学习</span>
            <strong>{formatStudyTime(stats.today_seconds)}</strong>
          </article>
          <article className="stat-card">
            <span>本周学习</span>
            <strong>{formatStudyTime(stats.week_seconds)}</strong>
          </article>
          <article className="stat-card">
            <span>本月学习</span>
            <strong>{formatStudyTime(stats.month_seconds)}</strong>
          </article>
          <article className="stat-card">
            <span>累计干扰</span>
            <strong>{stats.interruption_count} 次</strong>
          </article>
        </div>
      )}

      {focusEnforcementActive && latestAppCheck && (
        <div className={latestAppCheck.match_result.allowed ? 'monitor-card allowed' : 'monitor-card blocked'}>
          <div>
            <span>当前前台应用</span>
            <strong>{latestAppCheck.foreground_app.process_name}</strong>
            <p>{latestAppCheck.foreground_app.window_title || '无窗口标题'}</p>
          </div>
          <div>
            <span>{latestAppCheck.match_result.allowed ? '允许' : '干扰处理'}</span>
            <strong>{latestAppCheck.match_result.reason}</strong>
            {latestAppCheck.match_result.detected_domain && <p>识别网站：{latestAppCheck.match_result.detected_domain}</p>}
            <p>
              {latestAppCheck.match_result.allowed
                ? `累计干扰 ${latestAppCheck.interruption_count} 次`
                : latestAppCheck.close_error
                  ? `关闭失败：${latestAppCheck.close_error}`
                  : `已尝试关闭，累计干扰 ${latestAppCheck.interruption_count} 次`}
            </p>
          </div>
        </div>
      )}

      {pendingRecovery ? (
        <div className="recovery-panel">
          <div>
            <strong>发现未完成专注</strong>
            <p>
              {pendingRecovery.session.mode === 'strict' ? '严格模式' : '普通模式'} ·
              已进行 {formatDuration(pendingRecovery.elapsed_seconds)} ·
              剩余 {formatDuration(pendingRecovery.remaining_seconds)}
            </p>
          </div>
          <div className="emergency-actions">
            <button className="secondary-action" onClick={() => void handleInterruptRecoveredSession()} type="button">
              标记中断
            </button>
            <button className="secondary-action danger-solid" onClick={handleResumeRecoveredSession} type="button">
              继续专注
            </button>
          </div>
        </div>
      ) : studyPhase === 'focus' || studyPhase === 'awaiting_break' ? (
        <>
          <div className="action-group">
            {studyPhase === 'awaiting_break' ? (
              <button className="primary-action" onClick={() => void handleStartBreak()} type="button">
                确认开始休息
              </button>
            ) : (
              <button className="primary-action" disabled type="button">
                学习模式进行中，番茄钟强制执行
              </button>
            )}
            {activeSession?.mode === 'strict' && (
              <button className="secondary-action danger-outline" onClick={openEmergencyExitPanel} type="button">
                应急退出
              </button>
            )}
          </div>

          {emergencyExitOpen && activeSession?.mode === 'strict' && (
            <div className="emergency-panel">
              <div>
                <strong>应急退出会将本次严格模式专注记为提前结束</strong>
                <p>请等待冷静倒计时结束，并输入确认文本后再退出。</p>
              </div>
              <div className="emergency-countdown">
                <span>冷静倒计时</span>
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
                  className="secondary-action danger-solid"
                  disabled={emergencyCooldownRemaining > 0 || emergencyConfirmValue.trim() !== emergencyConfirmText}
                  onClick={() => void handleEmergencyExit()}
                  type="button"
                >
                  确认应急退出
                </button>
              </div>
            </div>
          )}
        </>
      ) : studyPhase === 'break' ? (
        <button className="primary-action" disabled type="button">休息中，结束后自动进入下一轮</button>
      ) : studyPhase === 'finished' || studyPhase === 'emergency_exited' || studyPhase === 'interrupted' ? (
        <button className="primary-action" onClick={() => void handleReset()} type="button">开始下一次学习模式</button>
      ) : (
        <button className="primary-action" onClick={() => void handleStart()} type="button">开始学习模式</button>
      )}

      <div className="history-section">
        <h3>最近专注记录</h3>
        {history.length === 0 ? (
          <p className="muted">暂无记录，完成一次专注后会显示在这里。</p>
        ) : (
          <div className="list-card">
            {history.map((session) => (
              <div className="list-row history-row" key={session.id}>
                <div>
                  <strong>{formatDuration(session.actual_seconds || session.planned_seconds)}</strong>
                  <p>{new Date(session.started_at).toLocaleString()}</p>
                  <p>
                    {subjectNameMap.get(session.subject_id ?? -1) ?? '未分类'} · {session.mode === 'strict' ? '严格模式' : '普通模式'}
                  </p>
                </div>
                <div className="history-meta">
                  <span className={session.status === 'finished' ? 'status enabled' : 'status'}>{session.status}</span>
                  <p>干扰 {session.interruption_count} 次</p>
                  {session.mode === 'strict' && <p>应急退出 {session.emergency_exit_count} 次</p>}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

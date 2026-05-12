import { useEffect, useMemo, useState } from 'react';
import { finishFocusSession, listFocusSessions, startFocusSession } from '../services/focusApi';
import type { FocusMode, FocusSession, FocusStatus } from '../types/focus';

const presetMinutes = [1, 25, 45, 60, 90];

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

export default function FocusPage() {
  const [minutes, setMinutes] = useState(25);
  const [mode, setMode] = useState<FocusMode>('normal');
  const [status, setStatus] = useState<FocusStatus>('idle');
  const [activeSession, setActiveSession] = useState<FocusSession | null>(null);
  const [remainingSeconds, setRemainingSeconds] = useState(minutes * 60);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [history, setHistory] = useState<FocusSession[]>([]);
  const [error, setError] = useState<string | null>(null);

  const plannedSeconds = minutes * 60;

  useEffect(() => {
    if (status === 'idle') {
      setRemainingSeconds(plannedSeconds);
    }
  }, [plannedSeconds, status]);

  useEffect(() => {
    void refreshHistory();
  }, []);

  useEffect(() => {
    if (status !== 'running' || startedAt === null || activeSession === null) {
      return;
    }

    const tick = async () => {
      const elapsedSeconds = Math.floor((Date.now() - startedAt) / 1000);
      const nextRemainingSeconds = Math.max(plannedSeconds - elapsedSeconds, 0);
      setRemainingSeconds(nextRemainingSeconds);

      if (nextRemainingSeconds === 0) {
        setStatus('finished');
        const finishedSession = await finishFocusSession(activeSession.id, plannedSeconds);
        setActiveSession(finishedSession);
        await refreshHistory();
      }
    };

    void tick();
    const intervalId = window.setInterval(() => void tick(), 1000);
    return () => window.clearInterval(intervalId);
  }, [activeSession, plannedSeconds, startedAt, status]);

  const summary = useMemo(() => {
    if (status === 'running') {
      return '专注进行中：当前阶段只完成倒计时和记录保存，防退出将在阶段 6 接入。';
    }

    if (status === 'finished') {
      return '本次专注已完成，记录已经保存到本地数据库。';
    }

    return mode === 'normal' ? '普通模式：防误退出，发现干扰时提醒。' : '严格模式：后续阶段启用更强约束和应急退出。';
  }, [mode, status]);

  async function refreshHistory() {
    try {
      setHistory(await listFocusSessions());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleStart() {
    try {
      setError(null);
      const session = await startFocusSession(plannedSeconds, mode);
      setActiveSession(session);
      setRemainingSeconds(plannedSeconds);
      setStartedAt(Date.now());
      setStatus('running');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleReset() {
    setStatus('idle');
    setActiveSession(null);
    setStartedAt(null);
    setRemainingSeconds(plannedSeconds);
    await refreshHistory();
  }

  return (
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 2 / 专注倒计时闭环</p>
        <h2>专注倒计时</h2>
        <p>完成专注后会自动保存到本地 SQLite 数据库。</p>
      </div>

      <div className="focus-panel">
        <div className="timer-display">{formatSeconds(remainingSeconds)}</div>
        <p className="muted">{status === 'running' ? '专注进行中' : '计划专注时长'}</p>
      </div>

      <div className="preset-grid">
        {presetMinutes.map((value) => (
          <button
            className={value === minutes ? 'chip active' : 'chip'}
            disabled={status === 'running'}
            key={value}
            onClick={() => setMinutes(value)}
            type="button"
          >
            {value} 分钟
          </button>
        ))}
      </div>

      <div className="mode-switch">
        <button className={mode === 'normal' ? 'mode active' : 'mode'} disabled={status === 'running'} onClick={() => setMode('normal')} type="button">
          普通模式
        </button>
        <button className={mode === 'strict' ? 'mode active' : 'mode'} disabled={status === 'running'} onClick={() => setMode('strict')} type="button">
          严格模式
        </button>
      </div>

      <p className="notice">{summary}</p>
      {error && <p className="error-text">{error}</p>}

      {status === 'running' ? (
        <button className="primary-action" disabled type="button">专注中，等待倒计时结束</button>
      ) : status === 'finished' ? (
        <button className="primary-action" onClick={() => void handleReset()} type="button">开始下一次专注</button>
      ) : (
        <button className="primary-action" onClick={() => void handleStart()} type="button">开始专注</button>
      )}

      <div className="history-section">
        <h3>最近专注记录</h3>
        {history.length === 0 ? (
          <p className="muted">暂无记录，完成一次专注后会显示在这里。</p>
        ) : (
          <div className="list-card">
            {history.map((session) => (
              <div className="list-row" key={session.id}>
                <div>
                  <strong>{formatDuration(session.actual_seconds || session.planned_seconds)}</strong>
                  <p>{new Date(session.started_at).toLocaleString()}</p>
                </div>
                <span className={session.status === 'finished' ? 'status enabled' : 'status'}>{session.status}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

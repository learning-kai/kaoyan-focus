import { useEffect, useMemo, useState } from 'react';
import { BarChart3, CalendarDays, Clock3, Pencil, RefreshCw, ShieldAlert, TimerReset, Trash2 } from 'lucide-react';
import { deleteFocusSession, getFocusStatsSummary, listFocusSessions, listSubjects, updateFocusSessionSubject } from '../services/focusApi';
import { listInterruptionSummary } from '../services/monitorApi';
import type { FocusSession, FocusStatsSummary, Subject } from '../types/focus';
import type { InterruptionSummary } from '../types/monitor';

function formatStudyTime(seconds: number) {
  if (seconds < 3600) {
    return `${Math.round(seconds / 60)} 分钟`;
  }

  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

export default function StatsPage() {
  const [stats, setStats] = useState<FocusStatsSummary | null>(null);
  const [interruptions, setInterruptions] = useState<InterruptionSummary[]>([]);
  const [sessions, setSessions] = useState<FocusSession[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [savingSessionId, setSavingSessionId] = useState<number | null>(null);
  const [deletingSessionId, setDeletingSessionId] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);

  useEffect(() => {
    void refreshStats();
  }, []);

  async function refreshStats() {
    try {
      setError(null);
      const [statsData, interruptionData, sessionData, subjectData] = await Promise.all([
        getFocusStatsSummary(),
        listInterruptionSummary(),
        listFocusSessions(),
        listSubjects(),
      ]);
      setStats(statsData);
      setInterruptions(interruptionData);
      setSessions(sessionData);
      setSubjects(subjectData);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleSubjectChange(sessionId: number, value: string) {
    try {
      setSavingSessionId(sessionId);
      setError(null);
      setMessage(null);
      const subjectId = value === '' ? null : Number(value);
      const updated = await updateFocusSessionSubject(sessionId, subjectId);
      setSessions((current) => current.map((session) => (session.id === sessionId ? updated : session)));
      const nextStats = await getFocusStatsSummary();
      setStats(nextStats);
      setMessage('学习记录科目已更新。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSavingSessionId(null);
    }
  }

  async function handleDeleteSession(sessionId: number) {
    if (!window.confirm('确定删除这条学习记录吗？删除后统计会同步更新。')) {
      return;
    }

    try {
      setDeletingSessionId(sessionId);
      setError(null);
      setMessage(null);
      await deleteFocusSession(sessionId);
      await refreshStats();
      setMessage('学习记录已删除。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setDeletingSessionId(null);
    }
  }

  return (
    <section className="page-shell stats-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">Local Analytics</p>
          <h2>学习统计</h2>
          <p>按今日、本周、本月和科目汇总本地专注记录，同时保留非白名单干扰排行。</p>
        </div>
        <button className="secondary-action" onClick={() => void refreshStats()} type="button">
          <RefreshCw size={17} />
          刷新
        </button>
      </header>

      {error && <p className="alert error">{error}</p>}
      {message && <p className="alert success">{message}</p>}

      <div className="stats-hero-grid">
        <MetricCard icon={Clock3} label="今日学习" value={formatStudyTime(stats?.today_seconds ?? 0)} />
        <MetricCard icon={CalendarDays} label="本周学习" value={formatStudyTime(stats?.week_seconds ?? 0)} />
        <MetricCard icon={BarChart3} label="本月学习" value={formatStudyTime(stats?.month_seconds ?? 0)} />
        <MetricCard danger icon={ShieldAlert} label="累计干扰" value={`${stats?.interruption_count ?? 0} 次`} />
      </div>

      <div className="stats-board">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Interruptions</p>
              <h3>干扰排行</h3>
            </div>
            <ShieldAlert size={20} />
          </div>

          {interruptions.length === 0 ? (
            <div className="empty-state">
              <strong>暂无干扰记录</strong>
              <p>专注期间检测到非白名单应用后，这里会显示最常打断你的软件。</p>
            </div>
          ) : (
            <div className="rule-list">
              {interruptions.map((item) => (
                <article className="list-row interruption-row" key={item.process_name}>
                  <div className="row-main">
                    <span className="row-icon danger"><ShieldAlert size={18} /></span>
                    <div>
                      <strong>{item.process_name}</strong>
                      <p>{item.window_title || '无窗口标题'}</p>
                      {item.process_path && <p>{item.process_path}</p>}
                      <p>最近：{new Date(item.last_interrupted_at).toLocaleString()}</p>
                    </div>
                  </div>
                  <strong className="count-pill">{item.interruption_count} 次</strong>
                </article>
              ))}
            </div>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Subjects</p>
              <h3>科目分布</h3>
            </div>
            <BarChart3 size={20} />
          </div>

          {!stats || stats.subjects.length === 0 ? (
            <div className="empty-state">
              <strong>暂无科目统计</strong>
              <p>开始一次带科目的专注后，这里会显示各科累计学习时长。</p>
            </div>
          ) : (
            <div className="subject-bars">
              {stats.subjects.map((item) => {
                const maxSeconds = Math.max(...stats.subjects.map((subject) => subject.total_seconds), 1);
                const width = Math.max(6, (item.total_seconds / maxSeconds) * 100);

                return (
                  <article className="subject-stat-row" key={item.subject.id}>
                    <div>
                      <span className="subject-dot" style={{ backgroundColor: item.subject.color ?? '#8fb5ff' }} />
                      <strong>{item.subject.name}</strong>
                      <small>{item.subject.enabled ? '已启用' : '已停用'}</small>
                    </div>
                    <div className="subject-bar">
                      <i style={{ width: `${width}%`, backgroundColor: item.subject.color ?? '#8fb5ff' }} />
                    </div>
                    <strong>{formatStudyTime(item.total_seconds)}</strong>
                  </article>
                );
              })}
            </div>
          )}
        </section>
      </div>

      <section className="command-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Records</p>
            <h3>最近学习记录</h3>
          </div>
          <Pencil size={20} />
        </div>

        {sessions.length === 0 ? (
          <div className="empty-state">
            <strong>暂无学习记录</strong>
            <p>完成一次番茄钟后，可以在这里补改科目。</p>
          </div>
        ) : (
          <div className="records-table">
            {sessions.map((session) => (
              <article className="record-row" key={session.id}>
                <div className="record-time">
                  <span className="row-icon enabled"><TimerReset size={18} /></span>
                  <div>
                    <strong>{formatStudyTime(session.actual_seconds || session.planned_seconds)}</strong>
                    <p>{new Date(session.started_at).toLocaleString()} / {sessionStatusLabel(session.status)}</p>
                  </div>
                </div>
                <div className="record-subject">
                  <span>{session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未知科目' : '未指定科目'}</span>
                  <div className="record-subject-actions">
                    <select
                      aria-label="修改记录科目"
                      className="select-input"
                      disabled={savingSessionId === session.id || deletingSessionId === session.id}
                      onChange={(event) => void handleSubjectChange(session.id, event.target.value)}
                      value={session.subject_id ?? ''}
                    >
                      <option value="">未指定</option>
                      {subjects.map((subject) => (
                        <option key={subject.id} value={subject.id}>{subject.name}</option>
                      ))}
                    </select>
                    <button
                      className="small-action danger"
                      disabled={deletingSessionId === session.id || savingSessionId === session.id}
                      onClick={() => void handleDeleteSession(session.id)}
                      type="button"
                    >
                      <Trash2 size={15} />
                      删除记录
                    </button>
                  </div>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}

function MetricCard({
  danger = false,
  icon: Icon,
  label,
  value,
}: {
  danger?: boolean;
  icon: typeof Clock3;
  label: string;
  value: string;
}) {
  return (
    <article className={danger ? 'metric-card large danger' : 'metric-card large'}>
      <Icon size={20} />
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function sessionStatusLabel(status: string) {
  const labels: Record<string, string> = {
    running: '进行中',
    finished: '已完成',
    interrupted: '已中断',
    emergency_exited: '已退出',
  };

  return labels[status] ?? status;
}

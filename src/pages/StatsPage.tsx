import { useEffect, useState } from 'react';
import { BarChart3, CalendarDays, Clock3, ShieldAlert, TimerReset } from 'lucide-react';
import { getFocusStatsSummary } from '../services/focusApi';
import { listInterruptionSummary } from '../services/monitorApi';
import type { FocusStatsSummary } from '../types/focus';
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
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshStats();
  }, []);

  async function refreshStats() {
    try {
      setError(null);
      const [statsData, interruptionData] = await Promise.all([
        getFocusStatsSummary(),
        listInterruptionSummary(),
      ]);
      setStats(statsData);
      setInterruptions(interruptionData);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="page-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">统计 / 本地记录</p>
          <h2>学习统计</h2>
          <p>按今日、本周、本月和科目汇总本地专注记录，同时保留非白名单干扰排行。</p>
        </div>
        <button className="secondary-action" onClick={() => void refreshStats()} type="button">
          <TimerReset size={17} />
          刷新
        </button>
      </header>

      {error && <p className="alert error">{error}</p>}

      <div className="stats-grid four">
        <article className="metric-card large">
          <Clock3 size={20} />
          <span>今日学习</span>
          <strong>{formatStudyTime(stats?.today_seconds ?? 0)}</strong>
        </article>
        <article className="metric-card large">
          <CalendarDays size={20} />
          <span>本周学习</span>
          <strong>{formatStudyTime(stats?.week_seconds ?? 0)}</strong>
        </article>
        <article className="metric-card large">
          <BarChart3 size={20} />
          <span>本月学习</span>
          <strong>{formatStudyTime(stats?.month_seconds ?? 0)}</strong>
        </article>
        <article className="metric-card large danger">
          <ShieldAlert size={20} />
          <span>累计干扰</span>
          <strong>{stats?.interruption_count ?? 0} 次</strong>
        </article>
      </div>

      <div className="content-grid two">
        <section className="panel">
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
            <div className="list-card">
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

        <section className="panel">
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
            <div className="list-card">
              {stats.subjects.map((item) => (
                <article className="list-row subject-stat-row" key={item.subject.id}>
                  <div className="row-main">
                    <span className="subject-dot" style={{ backgroundColor: item.subject.color ?? '#94a3b8' }} />
                    <div>
                      <strong>{item.subject.name}</strong>
                      <p>{item.subject.enabled ? '已启用' : '已停用'}</p>
                    </div>
                  </div>
                  <strong>{formatStudyTime(item.total_seconds)}</strong>
                </article>
              ))}
            </div>
          )}
        </section>
      </div>
    </section>
  );
}

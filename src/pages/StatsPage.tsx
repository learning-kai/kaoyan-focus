import { useEffect, useState } from 'react';
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
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 7 / 统计与科目</p>
        <h2>学习统计</h2>
        <p>按今日、本周、本月和科目维度汇总本地专注记录。</p>
      </div>

      {error && <p className="error-text">{error}</p>}

      <div className="stats-grid">
        <article className="stat-card">
          <span>今日学习</span>
          <strong>{formatStudyTime(stats?.today_seconds ?? 0)}</strong>
        </article>
        <article className="stat-card">
          <span>本周学习</span>
          <strong>{formatStudyTime(stats?.week_seconds ?? 0)}</strong>
        </article>
        <article className="stat-card">
          <span>本月学习</span>
          <strong>{formatStudyTime(stats?.month_seconds ?? 0)}</strong>
        </article>
        <article className="stat-card">
          <span>累计干扰</span>
          <strong>{stats?.interruption_count ?? 0} 次</strong>
        </article>
      </div>

      <div className="history-section">
        <h3>干扰排行</h3>
        {interruptions.length === 0 ? (
          <div className="empty-state">
            <strong>暂无干扰记录</strong>
            <p>专注期间检测到非白名单应用后，这里会显示最常打断你的软件。</p>
          </div>
        ) : (
          <div className="list-card">
            {interruptions.map((item) => (
              <div className="list-row interruption-row" key={item.process_name}>
                <div>
                  <strong>{item.process_name}</strong>
                  <p>{item.window_title || '无窗口标题'}</p>
                  {item.process_path && <p>{item.process_path}</p>}
                  <p>最近：{new Date(item.last_interrupted_at).toLocaleString()}</p>
                </div>
                <strong>{item.interruption_count} 次</strong>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="history-section">
        <h3>科目分布</h3>
        {!stats || stats.subjects.length === 0 ? (
          <div className="empty-state">
            <strong>暂无科目统计</strong>
            <p>开始一次带科目的专注后，这里会显示各科累计学习时长。</p>
          </div>
        ) : (
          <div className="list-card">
            {stats.subjects.map((item) => (
              <div className="list-row subject-stat-row" key={item.subject.id}>
                <div className="subject-stat-title">
                  <span className="subject-dot" style={{ backgroundColor: item.subject.color ?? '#94a3b8' }} />
                  <div>
                    <strong>{item.subject.name}</strong>
                    <p>{item.subject.enabled ? '已启用' : '已停用'}</p>
                  </div>
                </div>
                <strong>{formatStudyTime(item.total_seconds)}</strong>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

import { useEffect, useMemo, useState } from 'react';
import { ChevronLeft, ChevronRight, NotebookPen, RefreshCw, Save, Sparkles, Trash2 } from 'lucide-react';
import { deleteDailyReview, getDailyReviewPageData, saveDailyReview } from '../services/reviewApi';
import type { DailyReviewDraft, DailyReviewPageData } from '../types/review';

function todayString() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function shiftDate(value: string, days: number) {
  const date = new Date(`${value}T00:00:00`);
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function formatDuration(seconds: number) {
  if (seconds <= 0) return '0 分钟';
  if (seconds < 3600) return `${Math.round(seconds / 60)} 分钟`;
  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

function emptyDraft(date: string): DailyReviewDraft {
  return {
    reviewDate: date,
    summary: '',
    blockers: '',
    tomorrowFocus: '',
    moodScore: 3,
  };
}

export default function ReviewPage() {
  const [selectedDate, setSelectedDate] = useState(todayString());
  const [data, setData] = useState<DailyReviewPageData | null>(null);
  const [draft, setDraft] = useState<DailyReviewDraft>(() => emptyDraft(todayString()));
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh(selectedDate);
  }, [selectedDate]);

  const completionText = useMemo(() => {
    if (!data) return '暂无数据';
    const schedule = `${data.summary.schedule_completed}/${data.summary.schedule_total}`;
    const today = `${data.summary.today_completed}/${data.summary.today_total}`;
    return `课表 ${schedule} · 今日任务 ${today}`;
  }, [data]);

  async function refresh(date = selectedDate) {
    try {
      setError(null);
      const pageData = await getDailyReviewPageData(date);
      setData(pageData);
      setDraft({
        reviewDate: pageData.review_date,
        summary: pageData.review?.summary ?? '',
        blockers: pageData.review?.blockers ?? '',
        tomorrowFocus: pageData.review?.tomorrow_focus ?? '',
        moodScore: pageData.review?.mood_score ?? 3,
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleSave() {
    try {
      setSaving(true);
      setError(null);
      await saveDailyReview(draft);
      await refresh(draft.reviewDate);
      setMessage('复盘已保存。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!data?.review) return;
    if (!window.confirm('确定删除这天的复盘吗？')) return;
    try {
      setSaving(true);
      setError(null);
      await deleteDailyReview(data.review.id);
      await refresh(selectedDate);
      setMessage('复盘已删除。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="page-shell review-shell">
      <header className="review-hero">
        <div>
          <p className="eyebrow">Review</p>
          <h2>每日复盘</h2>
          <p>把今天的学习、课表和任务收成一页，明天打开时不用重新想从哪开始。</p>
        </div>
        <div className="review-date-tools">
          <button aria-label="前一天" className="ghost-action icon-action" type="button" onClick={() => setSelectedDate(shiftDate(selectedDate, -1))}>
            <ChevronLeft size={16} />
          </button>
          <input className="text-input" type="date" value={selectedDate} onChange={(event) => setSelectedDate(event.target.value)} />
          <button aria-label="后一天" className="ghost-action icon-action" type="button" onClick={() => setSelectedDate(shiftDate(selectedDate, 1))}>
            <ChevronRight size={16} />
          </button>
          <button className="ghost-action" type="button" onClick={() => void refresh()}>
            <RefreshCw size={16} /> 刷新
          </button>
        </div>
      </header>

      {(error || message) && <div className={error ? 'alert error' : 'alert success'}>{error ?? message}</div>}

      <div className="review-grid">
        <aside className="review-summary-panel soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Today Signal</p>
              <h3>{data?.review_date ?? selectedDate}</h3>
            </div>
            <Sparkles size={20} />
          </div>
          <div className="review-metric-grid">
            <Metric label="学习时长" value={formatDuration(data?.summary.study_seconds ?? 0)} />
            <Metric label="番茄记录" value={`${data?.summary.focus_session_count ?? 0} 条`} />
            <Metric label="干扰次数" value={`${data?.summary.interruption_count ?? 0} 次`} />
            <Metric label="执行进度" value={completionText} />
          </div>
        </aside>

        <section className="review-editor soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Daily Notes</p>
              <h3>今天留下什么</h3>
            </div>
            <NotebookPen size={20} />
          </div>

          <div className="review-score-row">
            <span>状态评分</span>
            {[1, 2, 3, 4, 5].map((score) => (
              <button className={draft.moodScore === score ? 'active' : ''} key={score} type="button" onClick={() => setDraft((current) => ({ ...current, moodScore: score }))}>
                {score}
              </button>
            ))}
          </div>

          <label className="field-block">
            <span>今日总结</span>
            <textarea className="text-input review-textarea" value={draft.summary ?? ''} onChange={(event) => setDraft((current) => ({ ...current, summary: event.target.value }))} placeholder="今天真正推进了什么？哪些安排有效？" />
          </label>
          <label className="field-block">
            <span>问题卡点</span>
            <textarea className="text-input review-textarea" value={draft.blockers ?? ''} onChange={(event) => setDraft((current) => ({ ...current, blockers: event.target.value }))} placeholder="卡住的题、分心原因、没有执行的原因。" />
          </label>
          <label className="field-block">
            <span>明日重点</span>
            <textarea className="text-input review-textarea" value={draft.tomorrowFocus ?? ''} onChange={(event) => setDraft((current) => ({ ...current, tomorrowFocus: event.target.value }))} placeholder="明天最先做哪几件事？" />
          </label>

          <div className="review-actions">
            <button className="primary-action" disabled={saving} type="button" onClick={() => void handleSave()}>
              <Save size={16} /> 保存复盘
            </button>
            <button className="small-action danger" disabled={saving || !data?.review} type="button" onClick={() => void handleDelete()}>
              <Trash2 size={15} /> 删除
            </button>
          </div>
        </section>
      </div>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <article className="review-metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

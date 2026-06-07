import { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, NotebookPen, RefreshCw, Save, Sparkles, Trash2 } from 'lucide-react';
import {
  deleteDailyReview,
  deleteWeeklyReview,
  getDailyReviewPageData,
  getWeeklyReviewPageData,
  saveDailyReview,
  saveWeeklyReview,
} from '../services/reviewApi';
import { syncConfiguredStateChange } from '../services/syncApi';
import type { DailyReviewDraft, DailyReviewPageData, WeeklyReviewDraft, WeeklyReviewPageData } from '../types/review';

type ReviewMode = 'daily' | 'weekly';

function todayString() {
  const date = new Date();
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function shiftDate(value: string, days: number) {
  const [year, month, day] = value.split('-').map(Number);
  if (!year || !month || !day) {
    return value;
  }

  const date = new Date(year, month - 1, day);
  date.setDate(date.getDate() + days);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function weekStartString(value: string) {
  const [year, month, day] = value.split('-').map(Number);
  if (!year || !month || !day) {
    return todayString();
  }
  const date = new Date(year, month - 1, day);
  const dayIndex = (date.getDay() + 6) % 7;
  date.setDate(date.getDate() - dayIndex);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function formatDuration(seconds: number) {
  if (seconds <= 0) return '0 分钟';
  if (seconds < 3600) return `${Math.round(seconds / 60)} 分钟`;
  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

function emptyDailyDraft(date: string): DailyReviewDraft {
  return {
    reviewDate: date,
    summary: '',
    blockers: '',
    tomorrowFocus: '',
    moodScore: 3,
  };
}

function emptyWeeklyDraft(weekStartDate: string): WeeklyReviewDraft {
  return {
    weekStartDate,
    summary: '',
    blockers: '',
    nextWeekFocus: '',
    moodScore: 3,
  };
}

export default function ReviewPage() {
  const [mode, setMode] = useState<ReviewMode>('daily');
  const [selectedDate, setSelectedDate] = useState(todayString());
  const [data, setData] = useState<DailyReviewPageData | null>(null);
  const [weeklyData, setWeeklyData] = useState<WeeklyReviewPageData | null>(null);
  const [draft, setDraft] = useState<DailyReviewDraft>(() => emptyDailyDraft(todayString()));
  const [weeklyDraft, setWeeklyDraft] = useState<WeeklyReviewDraft>(() => emptyWeeklyDraft(weekStartString(todayString())));
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (mode === 'daily') {
      void refreshDaily(selectedDate);
    } else {
      void refreshWeekly(selectedDate);
    }
  }, [mode, selectedDate]);

  async function refreshDaily(date = selectedDate) {
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

  async function refreshWeekly(date = selectedDate) {
    try {
      setError(null);
      const pageData = await getWeeklyReviewPageData(date);
      setWeeklyData(pageData);
      setWeeklyDraft({
        weekStartDate: pageData.week_start_date,
        summary: pageData.review?.summary ?? '',
        blockers: pageData.review?.blockers ?? '',
        nextWeekFocus: pageData.review?.next_week_focus ?? '',
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
      setMessage(null);
      if (mode === 'daily') {
        await saveDailyReview(draft);
        await refreshDaily(draft.reviewDate);
      } else {
        await saveWeeklyReview(weeklyDraft);
        await refreshWeekly(weeklyDraft.weekStartDate);
      }
      setMessage('复盘已保存。');
      void syncConfiguredStateChange('local_data_change').catch(() => undefined);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    const reviewId = mode === 'daily' ? data?.review?.id : weeklyData?.review?.id;
    if (!reviewId) return;
    if (!window.confirm(mode === 'daily' ? '确定删除这一天的复盘吗？' : '确定删除这一周的复盘吗？')) return;

    try {
      setSaving(true);
      setError(null);
      setMessage(null);
      if (mode === 'daily') {
        await deleteDailyReview(reviewId);
        await refreshDaily(selectedDate);
      } else {
        await deleteWeeklyReview(reviewId);
        await refreshWeekly(selectedDate);
      }
      setMessage('复盘已删除。');
      void syncConfiguredStateChange('local_data_change').catch(() => undefined);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  const activeSummary = mode === 'daily' ? data?.summary : weeklyData?.summary;
  const activeLabel = mode === 'daily'
    ? data?.review_date ?? selectedDate
    : `${weeklyData?.week_start_date ?? weekStartString(selectedDate)} ~ ${weeklyData?.week_end_date ?? shiftDate(weekStartString(selectedDate), 6)}`;

  return (
    <section className="page-shell review-shell">
      <header className="review-hero">
        <div>
          <p className="eyebrow">Review</p>
          <h2>{mode === 'daily' ? '每日复盘' : '周复盘'}</h2>
          <p>{mode === 'daily' ? '把今天的学习节奏、问题卡点和明日重点收拢起来。' : '按周一到周日复盘本周推进、卡点和下周重点。'}</p>
        </div>
        <div className="review-date-tools">
          <div className="segmented-control review-mode-toggle">
            <button className={mode === 'daily' ? 'active' : ''} type="button" onClick={() => setMode('daily')}>日复盘</button>
            <button className={mode === 'weekly' ? 'active' : ''} type="button" onClick={() => setMode('weekly')}>周复盘</button>
          </div>
          <button aria-label={mode === 'daily' ? '前一天' : '前一周'} className="ghost-action icon-action" type="button" onClick={() => setSelectedDate(shiftDate(selectedDate, mode === 'daily' ? -1 : -7))}>
            <ChevronLeft size={16} />
          </button>
          <input className="text-input" type="date" value={selectedDate} onChange={(event) => setSelectedDate(event.target.value)} />
          <button aria-label={mode === 'daily' ? '后一天' : '后一周'} className="ghost-action icon-action" type="button" onClick={() => setSelectedDate(shiftDate(selectedDate, mode === 'daily' ? 1 : 7))}>
            <ChevronRight size={16} />
          </button>
          <button className="ghost-action" type="button" onClick={() => mode === 'daily' ? void refreshDaily() : void refreshWeekly()}>
            <RefreshCw size={16} /> 刷新
          </button>
        </div>
      </header>

      {(error || message) && <div className={error ? 'alert error' : 'alert success'}>{error ?? message}</div>}

      <div className="review-grid">
        <aside className="review-summary-panel soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">{mode === 'daily' ? 'Daily Signal' : 'Weekly Signal'}</p>
              <h3>{activeLabel}</h3>
            </div>
            <Sparkles size={20} />
          </div>
          <div className="review-metric-grid">
            <Metric label="学习时长" value={formatDuration(activeSummary?.study_seconds ?? 0)} />
            <Metric label="番茄记录" value={`${activeSummary?.focus_session_count ?? 0} 条`} />
            <Metric label="干扰次数" value={`${activeSummary?.interruption_count ?? 0} 次`} />
          </div>
        </aside>

        <section className="review-editor soft-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">{mode === 'daily' ? 'Daily Notes' : 'Weekly Notes'}</p>
              <h3>{mode === 'daily' ? '今天留下什么' : '这一周沉淀什么'}</h3>
            </div>
            <NotebookPen size={20} />
          </div>

          <div className="review-score-row">
            <span>状态评分</span>
            {[1, 2, 3, 4, 5].map((score) => (
              <button
                className={(mode === 'daily' ? draft.moodScore : weeklyDraft.moodScore) === score ? 'active' : ''}
                key={score}
                type="button"
                onClick={() => {
                  if (mode === 'daily') {
                    setDraft((current) => ({ ...current, moodScore: score }));
                  } else {
                    setWeeklyDraft((current) => ({ ...current, moodScore: score }));
                  }
                }}
              >
                {score}
              </button>
            ))}
          </div>

          {mode === 'daily' ? (
            <>
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
            </>
          ) : (
            <>
              <label className="field-block">
                <span>本周总结</span>
                <textarea className="text-input review-textarea" value={weeklyDraft.summary ?? ''} onChange={(event) => setWeeklyDraft((current) => ({ ...current, summary: event.target.value }))} placeholder="这一周最重要的推进是什么？" />
              </label>
              <label className="field-block">
                <span>问题卡点</span>
                <textarea className="text-input review-textarea" value={weeklyDraft.blockers ?? ''} onChange={(event) => setWeeklyDraft((current) => ({ ...current, blockers: event.target.value }))} placeholder="这周反复卡住在哪里？" />
              </label>
              <label className="field-block">
                <span>下周重点</span>
                <textarea className="text-input review-textarea" value={weeklyDraft.nextWeekFocus ?? ''} onChange={(event) => setWeeklyDraft((current) => ({ ...current, nextWeekFocus: event.target.value }))} placeholder="下周最先守住哪几个重点？" />
              </label>
            </>
          )}

          <div className="review-actions">
            <button className="primary-action" disabled={saving} type="button" onClick={() => void handleSave()}>
              <Save size={16} /> 保存复盘
            </button>
            <button className="small-action danger" disabled={saving || (mode === 'daily' ? !data?.review : !weeklyData?.review)} type="button" onClick={() => void handleDelete()}>
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

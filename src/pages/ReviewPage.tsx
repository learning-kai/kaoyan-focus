import { useEffect, useRef, useState } from 'react';
import {
  CalendarDays,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  ListChecks,
  LoaderCircle,
  NotebookPen,
  RefreshCw,
  Save,
  Target,
  Timer,
  Trash2,
} from 'lucide-react';
import {
  deleteDailyReview,
  deleteWeeklyReview,
  getDailyReviewPageData,
  getWeeklyReviewPageData,
  saveDailyReview,
  saveWeeklyReview,
} from '../services/reviewApi';
import { syncConfiguredStateChange } from '../services/syncApi';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { formatDateKey } from '../utils/date';
import type { DailyReviewDraft, DailyReviewPageData, WeeklyReviewDraft, WeeklyReviewPageData } from '../types/review';

type ReviewMode = 'daily' | 'weekly';

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
    return formatDateKey();
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

function formatReviewDate(value: string) {
  const [year, month, day] = value.split('-').map(Number);
  if (!year || !month || !day) return value;
  const date = new Date(year, month - 1, day);
  return `${year} 年 ${month} 月 ${day} 日，${['周日', '周一', '周二', '周三', '周四', '周五', '周六'][date.getDay()]}`;
}

function formatWeekRange(start: string, end: string) {
  const [startYear, startMonth, startDay] = start.split('-').map(Number);
  const [endYear, endMonth, endDay] = end.split('-').map(Number);
  if (!startYear || !startMonth || !startDay || !endYear || !endMonth || !endDay) {
    return `${start} 至 ${end}`;
  }
  const endPrefix = startYear === endYear ? '' : `${endYear} 年 `;
  return `${startYear} 年 ${startMonth} 月 ${startDay} 日 - ${endPrefix}${endMonth} 月 ${endDay} 日`;
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
  const { confirm, confirmDialog } = useConfirmDialog();
  const [mode, setMode] = useState<ReviewMode>('daily');
  const [selectedDate, setSelectedDate] = useState(formatDateKey());
  const [data, setData] = useState<DailyReviewPageData | null>(null);
  const [weeklyData, setWeeklyData] = useState<WeeklyReviewPageData | null>(null);
  const [draft, setDraft] = useState<DailyReviewDraft>(() => emptyDailyDraft(formatDateKey()));
  const [weeklyDraft, setWeeklyDraft] = useState<WeeklyReviewDraft>(() =>
    emptyWeeklyDraft(weekStartString(formatDateKey())),
  );
  const [saving, setSaving] = useState(false);
  const [loadingReview, setLoadingReview] = useState(false);
  const [dailyDirty, setDailyDirty] = useState(false);
  const [weeklyDirty, setWeeklyDirty] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const dailyRefreshTokenRef = useRef(0);
  const weeklyRefreshTokenRef = useRef(0);

  useEffect(() => {
    if (mode === 'daily') {
      void refreshDaily(selectedDate);
    } else {
      void refreshWeekly(selectedDate);
    }
  }, [mode, selectedDate]);

  async function refreshDaily(date = selectedDate, showLoading = true) {
    const token = dailyRefreshTokenRef.current + 1;
    dailyRefreshTokenRef.current = token;
    try {
      if (showLoading) setLoadingReview(true);
      setError(null);
      const pageData = await getDailyReviewPageData(date);
      if (dailyRefreshTokenRef.current !== token) return;
      setData(pageData);
      setDraft({
        reviewDate: pageData.review_date,
        summary: pageData.review?.summary ?? '',
        blockers: pageData.review?.blockers ?? '',
        tomorrowFocus: pageData.review?.tomorrow_focus ?? '',
        moodScore: pageData.review?.mood_score ?? 3,
      });
      setDailyDirty(false);
    } catch (reason) {
      if (dailyRefreshTokenRef.current === token) {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    } finally {
      if (dailyRefreshTokenRef.current === token && showLoading) setLoadingReview(false);
    }
  }

  async function refreshWeekly(date = selectedDate, showLoading = true) {
    const token = weeklyRefreshTokenRef.current + 1;
    weeklyRefreshTokenRef.current = token;
    try {
      if (showLoading) setLoadingReview(true);
      setError(null);
      const pageData = await getWeeklyReviewPageData(date);
      if (weeklyRefreshTokenRef.current !== token) return;
      setWeeklyData(pageData);
      setWeeklyDraft({
        weekStartDate: pageData.week_start_date,
        summary: pageData.review?.summary ?? '',
        blockers: pageData.review?.blockers ?? '',
        nextWeekFocus: pageData.review?.next_week_focus ?? '',
        moodScore: pageData.review?.mood_score ?? 3,
      });
      setWeeklyDirty(false);
    } catch (reason) {
      if (weeklyRefreshTokenRef.current === token) {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    } finally {
      if (weeklyRefreshTokenRef.current === token && showLoading) setLoadingReview(false);
    }
  }

  function activeDraftDirty() {
    return mode === 'daily' ? dailyDirty : weeklyDirty;
  }

  async function confirmDiscardDraft() {
    if (!activeDraftDirty()) return true;

    return confirm({
      cancelLabel: '继续编辑',
      confirmLabel: '丢弃修改',
      message:
        mode === 'daily'
          ? '当前日复盘有未保存修改，继续操作会用选中日期的数据覆盖草稿。'
          : '当前周复盘有未保存修改，继续操作会用选中周的数据覆盖草稿。',
      title: '丢弃未保存复盘？',
      tone: 'danger',
    });
  }

  function clearFeedback() {
    setError(null);
    setMessage(null);
  }

  async function handleModeChange(nextMode: ReviewMode) {
    if (nextMode === mode || !(await confirmDiscardDraft())) return;
    if (mode === 'daily') setDailyDirty(false);
    else setWeeklyDirty(false);
    clearFeedback();
    setMode(nextMode);
  }

  async function handleDateChange(nextDate: string) {
    if (nextDate === selectedDate || !(await confirmDiscardDraft())) return;
    if (mode === 'daily') setDailyDirty(false);
    else setWeeklyDirty(false);
    clearFeedback();
    setSelectedDate(nextDate);
  }

  async function handleRefresh() {
    if (!(await confirmDiscardDraft())) return;
    clearFeedback();
    if (mode === 'daily') await refreshDaily();
    else await refreshWeekly();
  }

  function updateDailyDraft(patch: Partial<DailyReviewDraft>) {
    setDraft((current) => ({ ...current, ...patch }));
    setDailyDirty(true);
    setMessage(null);
  }

  function updateWeeklyDraft(patch: Partial<WeeklyReviewDraft>) {
    setWeeklyDraft((current) => ({ ...current, ...patch }));
    setWeeklyDirty(true);
    setMessage(null);
  }

  async function handleSave() {
    try {
      setSaving(true);
      clearFeedback();
      if (mode === 'daily') {
        await saveDailyReview(draft);
        await refreshDaily(draft.reviewDate, false);
        setDailyDirty(false);
      } else {
        await saveWeeklyReview(weeklyDraft);
        await refreshWeekly(weeklyDraft.weekStartDate, false);
        setWeeklyDirty(false);
      }
      setMessage('复盘已保存，记录已同步到本地数据。');
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
    const confirmed = await confirm({
      confirmLabel: '删除复盘',
      message:
        mode === 'daily'
          ? '删除后这一天的总结、卡点和明日重点会被清空。'
          : '删除后这一周的总结、卡点和下周重点会被清空。',
      title: mode === 'daily' ? '删除每日复盘？' : '删除周复盘？',
      tone: 'danger',
    });
    if (!confirmed) return;

    try {
      setSaving(true);
      clearFeedback();
      if (mode === 'daily') {
        await deleteDailyReview(reviewId);
        await refreshDaily(selectedDate, false);
        setDailyDirty(false);
      } else {
        await deleteWeeklyReview(reviewId);
        await refreshWeekly(selectedDate, false);
        setWeeklyDirty(false);
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
  const activeReview = mode === 'daily' ? data?.review : weeklyData?.review;
  const activeDate = data?.review_date ?? selectedDate;
  const activeWeekStart = weeklyData?.week_start_date ?? weekStartString(selectedDate);
  const activeWeekEnd = weeklyData?.week_end_date ?? shiftDate(activeWeekStart, 6);
  const activePeriodLabel =
    mode === 'daily' ? formatReviewDate(activeDate) : formatWeekRange(activeWeekStart, activeWeekEnd);
  const isDirty = activeDraftDirty();
  const isToday = selectedDate === formatDateKey();
  const dailyTotal = (data?.summary.schedule_total ?? 0) + (data?.summary.today_total ?? 0);
  const dailyCompleted = (data?.summary.schedule_completed ?? 0) + (data?.summary.today_completed ?? 0);
  const completionPercent = dailyTotal > 0 ? Math.round((dailyCompleted / dailyTotal) * 100) : 0;
  const moodScore = mode === 'daily' ? draft.moodScore : weeklyDraft.moodScore;
  const editorDisabled = loadingReview || saving;

  return (
    <section className="page-shell review-shell" aria-busy={loadingReview}>
      <header className="review-hero">
        <div className="review-hero-copy">
          <p className="eyebrow">学习回顾</p>
          <h2>{mode === 'daily' ? '每日复盘' : '周度复盘'}</h2>
          <p>{mode === 'daily' ? '收束今天的投入，为明天留下一条清楚的起点。' : '看清一周的投入与阻力，把下周的重点压缩到可执行。'}</p>
        </div>

        <div className="review-toolbar" aria-label="复盘时间与视图">
          <div className="segmented-control review-mode-toggle" aria-label="复盘视图">
            <button aria-pressed={mode === 'daily'} className={mode === 'daily' ? 'active' : ''} type="button" onClick={() => void handleModeChange('daily')}>
              日复盘
            </button>
            <button aria-pressed={mode === 'weekly'} className={mode === 'weekly' ? 'active' : ''} type="button" onClick={() => void handleModeChange('weekly')}>
              周复盘
            </button>
          </div>
          <div className="review-date-tools">
            <button aria-label={mode === 'daily' ? '前一天' : '前一周'} className="ghost-action icon-action" disabled={editorDisabled} title={mode === 'daily' ? '前一天' : '前一周'} type="button" onClick={() => void handleDateChange(shiftDate(selectedDate, mode === 'daily' ? -1 : -7))}>
              <ChevronLeft size={17} />
            </button>
            <label className="review-date-picker">
              <span className="sr-only">{mode === 'daily' ? '选择复盘日期' : '选择所在日期，自动归入对应周'}</span>
              <CalendarDays aria-hidden="true" size={16} />
              <input className="text-input" type="date" value={selectedDate} onChange={(event) => void handleDateChange(event.target.value)} />
            </label>
            <button aria-label={mode === 'daily' ? '后一天' : '后一周'} className="ghost-action icon-action" disabled={editorDisabled} title={mode === 'daily' ? '后一天' : '后一周'} type="button" onClick={() => void handleDateChange(shiftDate(selectedDate, mode === 'daily' ? 1 : 7))}>
              <ChevronRight size={17} />
            </button>
          </div>
          <button className="ghost-action review-today-action" disabled={editorDisabled || isToday} type="button" onClick={() => void handleDateChange(formatDateKey())}>
            今天
          </button>
          <button aria-label="重新读取复盘数据" className="ghost-action icon-action" disabled={editorDisabled} title="重新读取复盘数据" type="button" onClick={() => void handleRefresh()}>
            <RefreshCw className={loadingReview ? 'is-spinning' : ''} size={16} />
          </button>
        </div>
      </header>

      {error && (
        <div className="alert error review-feedback" role="alert">
          <CircleAlert aria-hidden="true" size={18} />
          <span>读取或保存复盘失败：{error}</span>
          <button className="small-action" disabled={saving || loadingReview} type="button" onClick={() => void handleRefresh()}>
            重试
          </button>
        </div>
      )}
      {message && !error && (
        <div className="alert success review-feedback" role="status" aria-live="polite">
          <CheckCircle2 aria-hidden="true" size={18} />
          <span>{message}</span>
        </div>
      )}
      {loadingReview && (
        <div className="review-loading" role="status" aria-live="polite">
          <LoaderCircle aria-hidden="true" className="is-spinning" size={17} /> 正在更新复盘数据…
        </div>
      )}
      {confirmDialog}

      <div className="review-context-row">
        <div>
          <p className="eyebrow">{mode === 'daily' ? '当日概览' : '本周概览'}</p>
          <h3>{activePeriodLabel}</h3>
        </div>
        <span className={isDirty ? 'review-draft-state is-dirty' : 'review-draft-state'} aria-live="polite">
          {isDirty ? '有未保存修改' : activeReview ? '已保存' : '尚未保存'}
        </span>
      </div>

      <div className="review-grid">
        <aside className="review-summary-panel soft-panel" aria-label={mode === 'daily' ? '当日数据概览' : '本周数据概览'}>
          <div className="panel-title">
            <div>
              <p className="eyebrow">投入记录</p>
              <h3>{mode === 'daily' ? '今天的学习信号' : '本周的学习信号'}</h3>
            </div>
            <Timer aria-hidden="true" size={20} />
          </div>
          <div className="review-metric-grid">
            <Metric label="学习时长" value={formatDuration(activeSummary?.study_seconds ?? 0)} />
            <Metric label="番茄记录" value={`${activeSummary?.focus_session_count ?? 0} 条`} />
            <Metric label="干扰次数" value={`${activeSummary?.interruption_count ?? 0} 次`} />
          </div>

          {mode === 'daily' ? (
            <section className="review-progress" aria-label="今日完成进度">
              <div className="review-progress-head">
                <div>
                  <p>今日完成</p>
                  <strong>{dailyTotal > 0 ? `${dailyCompleted} / ${dailyTotal}` : '暂无计划'}</strong>
                </div>
                <span>{dailyTotal > 0 ? `${completionPercent}%` : '—'}</span>
              </div>
              <div aria-label={dailyTotal > 0 ? `已完成 ${completionPercent}%` : '暂无可统计的计划'} aria-valuemax={100} aria-valuemin={0} aria-valuenow={completionPercent} className="review-progress-track" role="progressbar">
                <span style={{ width: `${completionPercent}%` }} />
              </div>
              <div className="review-progress-breakdown">
                <span><ListChecks aria-hidden="true" size={14} /> 日程 {data?.summary.schedule_completed ?? 0}/{data?.summary.schedule_total ?? 0}</span>
                <span><Target aria-hidden="true" size={14} /> 今日任务 {data?.summary.today_completed ?? 0}/{data?.summary.today_total ?? 0}</span>
              </div>
            </section>
          ) : (
            <p className="review-week-note">周视图仅汇总学习记录；计划完成情况请在对应日复盘中查看。</p>
          )}
        </aside>

        <section className="review-editor soft-panel" aria-label={mode === 'daily' ? '每日复盘编辑器' : '周复盘编辑器'}>
          <div className="review-editor-heading">
            <div className="panel-title">
              <div>
                <p className="eyebrow">写下复盘</p>
                <h3>{mode === 'daily' ? '留住今天的判断' : '沉淀这一周的判断'}</h3>
              </div>
              <NotebookPen aria-hidden="true" size={20} />
            </div>
            {!activeReview && !loadingReview && <p className="review-empty-note">还没有这段时间的复盘。写下几句，再保存即可。</p>}
          </div>

          <fieldset className="review-score-row" disabled={editorDisabled}>
            <legend>状态评分</legend>
            <span>凭直觉选一个分数即可</span>
            <div className="review-score-buttons">
              {[1, 2, 3, 4, 5].map((score) => (
                <button aria-label={`状态评分 ${score} 分`} aria-pressed={moodScore === score} className={moodScore === score ? 'active' : ''} key={score} type="button" onClick={() => (mode === 'daily' ? updateDailyDraft({ moodScore: score }) : updateWeeklyDraft({ moodScore: score }))}>
                  {score}
                </button>
              ))}
            </div>
          </fieldset>

          {mode === 'daily' ? (
            <>
              <ReviewField disabled={editorDisabled} hint="记录真正推进的内容，以及有效的安排。" id="daily-summary" label="今日总结" maxLength={1200} onChange={(summary) => updateDailyDraft({ summary })} placeholder="今天真正推进了什么？哪些安排有效？" value={draft.summary ?? ''} />
              <ReviewField disabled={editorDisabled} hint="把题目、节奏或注意力上的阻力说具体。" id="daily-blockers" label="问题卡点" maxLength={1200} onChange={(blockers) => updateDailyDraft({ blockers })} placeholder="卡住的题、分心原因、没有执行的原因。" value={draft.blockers ?? ''} />
              <ReviewField disabled={editorDisabled} hint="只写最值得先完成的几件事。" id="daily-focus" label="明日重点" maxLength={1200} onChange={(tomorrowFocus) => updateDailyDraft({ tomorrowFocus })} placeholder="明天最先做哪几件事？" value={draft.tomorrowFocus ?? ''} />
            </>
          ) : (
            <>
              <ReviewField disabled={editorDisabled} hint="归纳这周最重要的进展，而不是流水账。" id="weekly-summary" label="本周总结" maxLength={1200} onChange={(summary) => updateWeeklyDraft({ summary })} placeholder="这一周最重要的推进是什么？" value={weeklyDraft.summary ?? ''} />
              <ReviewField disabled={editorDisabled} hint="识别这一周反复出现的阻力。" id="weekly-blockers" label="问题卡点" maxLength={1200} onChange={(blockers) => updateWeeklyDraft({ blockers })} placeholder="这周反复卡住在哪里？" value={weeklyDraft.blockers ?? ''} />
              <ReviewField disabled={editorDisabled} hint="把下周的方向压缩到少数明确优先项。" id="weekly-focus" label="下周重点" maxLength={1200} onChange={(nextWeekFocus) => updateWeeklyDraft({ nextWeekFocus })} placeholder="下周最先守住哪几个重点？" value={weeklyDraft.nextWeekFocus ?? ''} />
            </>
          )}

          <div className="review-actions">
            <button className="primary-action" disabled={editorDisabled} type="button" onClick={() => void handleSave()}>
              <Save size={16} /> {saving ? '正在保存…' : isDirty ? '保存修改' : '保存复盘'}
            </button>
            <button className="small-action danger" disabled={editorDisabled || !activeReview} type="button" onClick={() => void handleDelete()}>
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

function ReviewField({
  disabled,
  hint,
  id,
  label,
  maxLength,
  onChange,
  placeholder,
  value,
}: {
  disabled: boolean;
  hint: string;
  id: string;
  label: string;
  maxLength: number;
  onChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  const hintId = `${id}-hint`;
  return (
    <label className="field-block review-field" htmlFor={id}>
      <span className="review-field-label">{label}</span>
      <span className="review-field-hint" id={hintId}>{hint}</span>
      <textarea aria-describedby={hintId} className="text-input review-textarea" disabled={disabled} id={id} maxLength={maxLength} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} value={value} />
      <span className="review-character-count">{value.length}/{maxLength}</span>
    </label>
  );
}

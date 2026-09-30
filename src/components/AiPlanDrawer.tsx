import { useCallback, useEffect, useMemo, useState } from 'react';
import type { KeyboardEvent } from 'react';
import {
  AlertTriangle,
  CalendarClock,
  CheckCircle2,
  ChevronDown,
  Loader2,
  MessageSquareText,
  RefreshCw,
  Send,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  X,
} from 'lucide-react';
import AiPlanTimeline from './AiPlanTimeline';
import {
  applyAiPlanProposal,
  discardAiPlanProposal,
  getLatestAiPlanProposal,
  parseAiSchedulerError,
  previewAiSchedule,
  regenerateAiPlanProposal,
  updateAiPlanProposalItems,
} from '../services/aiSchedulerApi';
import type { AiApplyResult, AiPlanItem, AiPlanProposal, AiSchedulerError } from '../types/aiScheduler';
import {
  AI_PLAN_DRAWER_PREFS_STORAGE_KEY,
  HORIZON_OPTIONS,
  MAX_EXTRA_INSTRUCTION_CHARS,
  MAX_FEEDBACK_CHARS,
  QUICK_FEEDBACK,
  QUICK_PROMPTS,
  applyResultTone,
  canFallbackToLocal,
  dateHeading,
  engineLabel,
  generalWarnings,
  hasQuickPrompt,
  isQuickPromptBlocked,
  loadingHint,
  localDateKey,
  parseAiPlanDrawerPrefs,
  removeItemFromDraft,
  serializeAiPlanDrawerPrefs,
  summarizeProposal,
  toggleQuickPrompt,
} from '../utils/aiPlanDrawer';
import type { AiPlanDrawerPrefs } from '../utils/aiPlanDrawer';

/**
 * AI 排期抽屉：选范围 → 生成草案 → 看摘要与每条理由（被自动调整过的会标出来）
 * → 删条目或写一句反馈让 AI 改 → 写入日历。
 *
 * 默认只走真模型（`allow_local_fallback = false`）：失败就报错，绝不悄悄用本地规则冒充 AI。
 * 只有用户在错误卡片里点「改用本地排期」，这一次才带 `allow_local_fallback = true`。
 */

type AiPlanDrawerProps = {
  isOpen: boolean;
  /** 规划的起始日期，YYYY-MM-DD */
  targetDate: string;
  /** 分类显示名（来自宿主页面的清单数据），用于把 category_key 显示成中文 */
  categoryLabels?: Record<string, string>;
  onClose: () => void;
  /** 写入成功后通知宿主页面刷新清单 / 日历 */
  onApplied: () => void;
};

/** 最近一次失败的操作类型：决定「重试」「改用本地排期」是否出现。 */
type LastAction = 'generate' | 'revise' | 'other';
type Mutation = 'remove' | 'apply' | 'discard' | null;

function readStoredPrefs(): AiPlanDrawerPrefs {
  try {
    return parseAiPlanDrawerPrefs(window.localStorage.getItem(AI_PLAN_DRAWER_PREFS_STORAGE_KEY));
  } catch {
    return parseAiPlanDrawerPrefs(null);
  }
}

export default function AiPlanDrawer({
  isOpen,
  targetDate,
  categoryLabels,
  onClose,
  onApplied,
}: AiPlanDrawerProps) {
  const [proposal, setProposal] = useState<AiPlanProposal | null>(null);
  const [prefs, setPrefs] = useState<AiPlanDrawerPrefs>(readStoredPrefs);
  const [extraInstruction, setExtraInstruction] = useState('');
  /** 会删旧块，不持久化：每次打开抽屉都要重新勾选。 */
  const [replaceAiBlocks, setReplaceAiBlocks] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(true);
  const [feedback, setFeedback] = useState('');
  const [generating, setGenerating] = useState(false);
  const [revising, setRevising] = useState(false);
  const [mutation, setMutation] = useState<Mutation>(null);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<AiSchedulerError | null>(null);
  const [lastAction, setLastAction] = useState<LastAction>('generate');
  const [applyResult, setApplyResult] = useState<AiApplyResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const thinking = generating || revising;
  const busy = thinking || mutation !== null;
  const today = localDateKey();
  const summary = useMemo(() => (proposal ? summarizeProposal(proposal) : null), [proposal]);
  const isDraft = proposal?.status === 'draft';
  const hint = loadingHint(elapsed, revising);
  const showRetry = Boolean(error?.retryable) && lastAction !== 'other';
  const showLocalFallback = lastAction === 'generate' && canFallbackToLocal(error);
  const topWarnings = proposal ? generalWarnings(proposal.warnings, proposal.items) : [];

  useEffect(() => {
    try {
      window.localStorage.setItem(AI_PLAN_DRAWER_PREFS_STORAGE_KEY, serializeAiPlanDrawerPrefs(prefs));
    } catch {
      // 偏好存不下不影响排期。
    }
  }, [prefs]);

  // 模型通常要 10–30 秒：计时让用户知道还在跑，而不是卡住了。
  useEffect(() => {
    if (!thinking) {
      setElapsed(0);
      return undefined;
    }
    const startedAt = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [thinking]);

  /** 打开时恢复上一次没确认的草案，避免切页回来就丢了现场。 */
  const restoreDraft = useCallback(async () => {
    try {
      const latest = await getLatestAiPlanProposal(targetDate);
      setProposal(latest);
      setOptionsOpen(latest === null);
    } catch (reason) {
      // 恢复失败不该堵住抽屉：用户仍然可以重新生成。
      setLastAction('other');
      setError(parseAiSchedulerError(reason));
    }
  }, [targetDate]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setError(null);
    setApplyResult(null);
    setNotice(null);
    setFeedback('');
    setReplaceAiBlocks(false);
    void restoreDraft();
  }, [isOpen, restoreDraft]);

  function updatePrefs(patch: Partial<AiPlanDrawerPrefs>) {
    setPrefs((current) => ({ ...current, ...patch }));
  }

  /**
   * `useLocal` 显式传参：旧实现先 setState 再调用，读到的是本次渲染的旧值，
   * 第一次点「改用本地排期」其实还在请求模型。
   */
  async function handleGenerate(useLocal = false) {
    setGenerating(true);
    setLastAction('generate');
    setError(null);
    setApplyResult(null);
    setNotice(null);
    try {
      const next = await previewAiSchedule({
        target_date: targetDate,
        horizon_days: prefs.horizonDays,
        queue_item_ids: null,
        category_keys: null,
        respect_priority: prefs.respectPriority,
        keep_locked_blocks: true,
        replace_ai_blocks: replaceAiBlocks,
        extra_instruction: extraInstruction.trim() || null,
        allow_local_fallback: useLocal,
      });
      setProposal(next);
      setOptionsOpen(false);
      setFeedback('');
      if (next.items.length === 0) {
        setNotice('这次没有排出任何条目，可以放宽条件后再试。');
      }
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setGenerating(false);
    }
  }

  /** 反馈只能交给模型：本地规则读不懂自然语言。失败时原草案保持不变。 */
  async function handleRevise(text: string = feedback) {
    const trimmed = text.trim();
    if (!proposal || !trimmed) {
      return;
    }
    setRevising(true);
    setLastAction('revise');
    setFeedback(trimmed);
    setError(null);
    setApplyResult(null);
    setNotice(null);
    try {
      setProposal(await regenerateAiPlanProposal(proposal.id, trimmed));
      setFeedback('');
      setNotice(`已按「${trimmed}」调整`);
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setRevising(false);
    }
  }

  function handleRetry() {
    if (lastAction === 'revise') {
      void handleRevise();
    } else {
      void handleGenerate();
    }
  }

  async function handleRemove(item: AiPlanItem) {
    if (!proposal) {
      return;
    }
    setMutation('remove');
    setLastAction('other');
    setError(null);
    setNotice(null);
    try {
      setProposal(await updateAiPlanProposalItems(proposal.id, removeItemFromDraft(proposal.items, item)));
      setNotice(item.kind === 'meal' ? `已去掉「${item.title}」` : `已删除「${item.title}」，它会出现在「没排上的任务」里`);
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setMutation(null);
    }
  }

  async function handleApply() {
    if (!proposal) {
      return;
    }
    setMutation('apply');
    setLastAction('other');
    setError(null);
    setNotice(null);
    try {
      const result = await applyAiPlanProposal(proposal.id, {
        overwrite_conflicts: false,
        skip_locked: true,
      });
      setApplyResult(result);
      setProposal(null);
      setOptionsOpen(true);
      onApplied();
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setMutation(null);
    }
  }

  async function handleDiscard() {
    if (!proposal) {
      return;
    }
    setMutation('discard');
    setLastAction('other');
    setError(null);
    try {
      await discardAiPlanProposal(proposal.id);
      setProposal(null);
      setApplyResult(null);
      setOptionsOpen(true);
      setNotice('已放弃这次草案，日历没有发生任何变化。');
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setMutation(null);
    }
  }

  function handleFeedbackKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter 发送、Shift+Enter 换行；输入法组字时的 Enter 不算。
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing && !busy) {
      event.preventDefault();
      void handleRevise();
    }
  }

  return (
    <section
      aria-hidden={!isOpen ? true : undefined}
      className={`schedule-drawer ai-plan-drawer${isOpen ? ' is-open' : ''}`}
      inert={!isOpen ? true : undefined}
    >
      <div className="panel-title schedule-drawer-head">
        <div>
          <p className="eyebrow">AI 排期</p>
          <h3>智能日程草案</h3>
        </div>
        <div className="today-drawer-tools">
          <button
            aria-label="关闭 AI 排期"
            className="focus-hud-card today-drawer-tool-card today-drawer-tool-icon"
            onClick={onClose}
            title="关闭"
            type="button"
          >
            <X size={15} />
          </button>
        </div>
      </div>

      <div className="today-plan-meta">
        <span>{dateHeading(targetDate, today)}</span>
        <span>{summary ? `${summary.scheduledCount} 条学习` : '尚未生成'}</span>
        {proposal && <span>{engineLabel(proposal.engine, proposal.degraded, proposal.model)}</span>}
      </div>

      {proposal && summary && (
        <div className="ai-plan-summary">
          <p className="ai-plan-summary-headline">
            <Sparkles size={13} /> {summary.headline}
          </p>
          {summary.progress !== null && (
            <div
              aria-label="学习时长占目标的比例"
              aria-valuemax={100}
              aria-valuemin={0}
              aria-valuenow={Math.round(summary.progress * 100)}
              className="ai-plan-progress"
              role="progressbar"
            >
              <span style={{ width: `${Math.round(summary.progress * 100)}%` }} />
            </div>
          )}
          <small>{summary.progressLabel}</small>
          <div className="ai-plan-stats">
            <span>{summary.scheduledCount} 条已排</span>
            {summary.unscheduledCount > 0 && (
              <span className="is-warn">{summary.unscheduledCount} 条没排上</span>
            )}
            {summary.adjustedCount > 0 && (
              <span className="is-info">{summary.adjustedCount} 条已自动调整</span>
            )}
            {summary.replaceableCount > 0 && (
              <span className="is-warn">写入时替换 {summary.replaceableCount} 条旧安排</span>
            )}
          </div>
          {summary.alreadyScheduledCount > 0 && (
            <details className="ai-plan-already">
              <summary>
                <CalendarClock size={12} /> {summary.alreadyScheduledCount} 条已在日历上，这次没有重复排
              </summary>
              {(proposal.already_scheduled ?? []).map((entry) => (
                <small key={entry.item_id}>
                  {entry.title} · {entry.detail}
                </small>
              ))}
            </details>
          )}
        </div>
      )}

      <button
        aria-controls="ai-plan-options"
        aria-expanded={optionsOpen}
        className="ghost-action ai-plan-options-toggle"
        onClick={() => setOptionsOpen((open) => !open)}
        type="button"
      >
        <SlidersHorizontal size={14} /> 排期选项
        <ChevronDown className={optionsOpen ? 'is-open' : undefined} size={14} />
      </button>

      {optionsOpen && (
        <div className="ai-plan-options" id="ai-plan-options">
          <p className="ai-plan-hint">
            排期范围＝「{targetDate}」当天计划队列里未完成的条目；已在日历上的不会重复排，锁定和手动安排的日程始终保留。
          </p>
          <label className="ai-plan-field">
            <span>规划范围</span>
            <select
              className="text-input"
              disabled={busy}
              onChange={(event) => updatePrefs({ horizonDays: Number(event.target.value) })}
              value={prefs.horizonDays}
            >
              {HORIZON_OPTIONS.map((days) => (
                <option key={days} value={days}>
                  {days === 1 ? '仅当天' : `连续 ${days} 天`}
                </option>
              ))}
            </select>
          </label>
          <div className="ai-plan-field">
            <span className="ai-plan-field-head">
              <label htmlFor="ai-plan-instruction">补充说明（可选）</label>
              <small>
                {extraInstruction.length}/{MAX_EXTRA_INSTRUCTION_CHARS}
              </small>
            </span>
            <div aria-label="快捷补充说明" className="ai-plan-chips" role="group">
              {QUICK_PROMPTS.map((prompt) => {
                const active = hasQuickPrompt(extraInstruction, prompt);
                return (
                  <button
                    aria-pressed={active}
                    className={`ai-plan-chip${active ? ' is-active' : ''}`}
                    disabled={busy || isQuickPromptBlocked(extraInstruction, prompt)}
                    key={prompt}
                    onClick={() => setExtraInstruction((text) => toggleQuickPrompt(text, prompt))}
                    type="button"
                  >
                    {prompt}
                  </button>
                );
              })}
            </div>
            <textarea
              className="text-input ai-plan-instruction"
              disabled={busy}
              id="ai-plan-instruction"
              maxLength={MAX_EXTRA_INSTRUCTION_CHARS}
              onChange={(event) => setExtraInstruction(event.target.value)}
              placeholder="例如：上午先做数学，晚上留出背单词的时间"
              rows={2}
              value={extraInstruction}
            />
          </div>
          <label className="ai-plan-check">
            <input
              checked={prefs.respectPriority}
              disabled={busy}
              onChange={(event) => updatePrefs({ respectPriority: event.target.checked })}
              type="checkbox"
            />
            <span>优先按优先级排序</span>
          </label>
          <label className="ai-plan-check">
            <input
              checked={replaceAiBlocks}
              disabled={busy}
              onChange={(event) => setReplaceAiBlocks(event.target.checked)}
              type="checkbox"
            />
            <span>重新安排已写入日历的 AI 日程（写入时替换旧安排）</span>
          </label>
          <button
            className="primary-action"
            disabled={busy}
            onClick={() => void handleGenerate()}
            type="button"
          >
            {generating ? <Loader2 className="spin" size={15} /> : <Sparkles size={15} />}
            {proposal ? '重新生成草案' : '生成草案'}
          </button>
        </div>
      )}

      {thinking && (
        <div className="ai-plan-alert is-info">
          <Loader2 className="spin" size={14} />
          <div>
            <strong aria-live="polite" role="status">
              {hint.title}
            </strong>
            <small aria-hidden="true">{hint.detail}</small>
          </div>
        </div>
      )}

      {error && (
        <div className="ai-plan-alert is-error" role="alert">
          <AlertTriangle size={14} />
          <div>
            <strong>{error.message}</strong>
            {error.degraded && <small>{error.degraded.description}</small>}
            {(showRetry || showLocalFallback) && (
              <div className="ai-plan-alert-actions">
                {showRetry && (
                  <button className="ghost-action ai-plan-retry" disabled={busy} onClick={handleRetry} type="button">
                    <RefreshCw size={13} /> 重试
                  </button>
                )}
                {showLocalFallback && (
                  <button
                    className="ghost-action ai-plan-retry"
                    disabled={busy}
                    onClick={() => void handleGenerate(true)}
                    title="AI 暂时不可用时的替代方案：改用本地规则排期，结果会标明「本地兜底」"
                    type="button"
                  >
                    <Sparkles size={13} /> 改用本地排期
                  </button>
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {notice && !error && !thinking && (
        <div className="ai-plan-alert is-info" role="status">
          <CheckCircle2 size={14} />
          <div>
            <strong>{notice}</strong>
          </div>
        </div>
      )}

      {applyResult && !error && (
        <div className={`ai-plan-alert is-${applyResultTone(applyResult)}`} role="status">
          {applyResultTone(applyResult) === 'info' ? <CheckCircle2 size={14} /> : <AlertTriangle size={14} />}
          <div>
            <strong>{applyResult.message}</strong>
            {applyResult.warnings.map((warning, index) => (
              <small key={`${warning.code}-${index}`}>{warning.message}</small>
            ))}
          </div>
        </div>
      )}

      <div className="ai-plan-body">
        {topWarnings.length > 0 && (
          <div className="ai-plan-alert is-warn">
            <AlertTriangle size={14} />
            <div>
              {topWarnings.map((warning, index) => (
                <small key={`${warning.code}-${index}`}>{warning.message}</small>
              ))}
            </div>
          </div>
        )}
        {proposal ? (
          <AiPlanTimeline
            categoryLabels={categoryLabels}
            items={proposal.items}
            onRemoveItem={isDraft ? (item) => void handleRemove(item) : undefined}
            removeDisabled={busy}
            today={today}
            unscheduled={proposal.unscheduled}
            warnings={proposal.warnings}
          />
        ) : (
          <div className="empty-state compact ai-plan-empty">
            <Sparkles size={24} />
            <strong>{applyResult && applyResult.created_count > 0 ? '已写入日历' : '还没有草案'}</strong>
            <p>
              {applyResult && applyResult.created_count > 0
                ? '可以去日历页查看；想再排一份，直接点「生成草案」。'
                : '选好范围后点「生成草案」，先看看效果再决定要不要写进日历。'}
            </p>
          </div>
        )}
      </div>

      {proposal && isDraft && (
        <div className="ai-plan-feedback">
          <label className="ai-plan-feedback-label" htmlFor="ai-plan-feedback">
            <MessageSquareText size={13} /> 不满意？告诉 AI 怎么改
          </label>
          <div aria-label="快捷反馈" className="ai-plan-chips" role="group">
            {QUICK_FEEDBACK.map((text) => (
              <button className="ai-plan-chip" disabled={busy} key={text} onClick={() => void handleRevise(text)} type="button">
                {text}
              </button>
            ))}
          </div>
          <div className="ai-plan-feedback-row">
            <textarea
              className="text-input"
              disabled={busy}
              id="ai-plan-feedback"
              maxLength={MAX_FEEDBACK_CHARS}
              onChange={(event) => setFeedback(event.target.value)}
              onKeyDown={handleFeedbackKeyDown}
              placeholder="例如：数学挪到下午，晚上别排太满（Enter 发送）"
              rows={2}
              value={feedback}
            />
            <button
              aria-label="按反馈调整"
              className="primary-action ai-plan-feedback-send"
              disabled={busy || !feedback.trim()}
              onClick={() => void handleRevise()}
              title="按反馈调整"
              type="button"
            >
              {revising ? <Loader2 className="spin" size={15} /> : <Send size={15} />}
            </button>
          </div>
        </div>
      )}

      {proposal && (
        <div className="ai-plan-actions">
          <button className="ghost-action" disabled={busy} onClick={() => void handleDiscard()} type="button">
            {mutation === 'discard' ? <Loader2 className="spin" size={15} /> : <Trash2 size={15} />} 放弃
          </button>
          <button className="primary-action" disabled={busy || !isDraft || proposal.items.length === 0} onClick={() => void handleApply()} type="button">
            {mutation === 'apply' ? <Loader2 className="spin" size={15} /> : <CheckCircle2 size={15} />}
            写入日历{summary && summary.scheduledCount > 0 ? `（${summary.scheduledCount} 条）` : ''}
          </button>
        </div>
      )}
    </section>
  );
}

import { useCallback, useEffect, useState } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  RefreshCw,
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
} from '../services/aiSchedulerApi';
import type {
  AiApplyResult,
  AiPlanProposal,
  AiSchedulerError,
} from '../types/aiScheduler';

/**
 * AI 排期抽屉。
 *
 * 默认**只走真模型**（`allow_local_fallback = false`）：AI 调用失败就报错，
 * 绝不悄悄用本地启发式冒充 AI 排期。只有用户在错误卡片里明确点了
 * 「改用本地排期」，下一次生成才会带 `allow_local_fallback = true`，
 * 且降级草案会标出「本地兜底」。
 *
 * 拖拽微调属于 S5，届时在 `AiPlanTimeline` 上加移动回调即可。
 */

const HORIZON_OPTIONS = [1, 2, 3, 5, 7];

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

export default function AiPlanDrawer({
  isOpen,
  targetDate,
  categoryLabels,
  onClose,
  onApplied,
}: AiPlanDrawerProps) {
  const [proposal, setProposal] = useState<AiPlanProposal | null>(null);
  const [horizonDays, setHorizonDays] = useState(1);
  const [keepLockedBlocks, setKeepLockedBlocks] = useState(true);
  const [respectPriority, setRespectPriority] = useState(true);
  const [extraInstruction, setExtraInstruction] = useState('');
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<AiSchedulerError | null>(null);
  const [applyResult, setApplyResult] = useState<AiApplyResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  /**
   * 仅对「下一次生成」生效的降级开关：默认 false（只信 AI），
   * 点了错误卡片里的「改用本地排期」才置 true，成功后立即复位，
   * 保证「重新生成草案」永远先试真模型。
   */
  const [allowLocalFallback, setAllowLocalFallback] = useState(false);

  const busy = loading || applying;

  /** 打开时恢复上一次没确认的草案，避免切页回来就丢了现场。 */
  const restoreDraft = useCallback(async () => {
    try {
      const latest = await getLatestAiPlanProposal(targetDate);
      setProposal(latest);
    } catch (reason) {
      // 恢复失败不该堵住抽屉：用户仍然可以点「生成草案」重新来一次。
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
    setAllowLocalFallback(false);
    void restoreDraft();
  }, [isOpen, restoreDraft]);

  async function handleGenerate() {
    setLoading(true);
    setError(null);
    setApplyResult(null);
    setNotice(null);
    try {
      const next = await previewAiSchedule({
        target_date: targetDate,
        horizon_days: horizonDays,
        queue_item_ids: null,
        category_keys: null,
        respect_priority: respectPriority,
        keep_locked_blocks: keepLockedBlocks,
        extra_instruction: extraInstruction.trim() ? extraInstruction.trim() : null,
        allow_local_fallback: allowLocalFallback,
      });
      setProposal(next);
      // 降级是一次性的：成功后复位，下次「重新生成」仍先试真模型。
      setAllowLocalFallback(false);
      if (next.items.length === 0) {
        setNotice('这次没有排出任何条目，可以在下方放宽条件后再试。');
      }
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleApply() {
    if (!proposal) {
      return;
    }
    setApplying(true);
    setError(null);
    try {
      const result = await applyAiPlanProposal(proposal.id, {
        overwrite_conflicts: false,
        skip_locked: true,
      });
      setApplyResult(result);
      setProposal(null);
      onApplied();
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setApplying(false);
    }
  }

  async function handleDiscard() {
    if (!proposal) {
      return;
    }
    setApplying(true);
    setError(null);
    try {
      await discardAiPlanProposal(proposal.id);
      setProposal(null);
      setApplyResult(null);
      setNotice('已放弃这次草案，日历没有发生任何变化。');
    } catch (reason) {
      setError(parseAiSchedulerError(reason));
    } finally {
      setApplying(false);
    }
  }

  const blockingWarnings = (proposal?.warnings ?? []).filter((warning) => !warning.item_id);

  /** 引擎徽标文案：正常 AI / 显式降级 / 本地（历史草案）。 */
  function engineChipText(engine: string, degraded: boolean, model: string): string {
    if (engine === 'llm') {
      return `AI 排期 · ${model}`;
    }
    return degraded ? '本地兜底排期（AI 不可用时的降级结果）' : '本地排期（未使用 AI）';
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
        <span>{targetDate}</span>
        <span>{proposal ? `${proposal.items.length} 条草案` : '尚未生成'}</span>
        {proposal && (
          <span>{engineChipText(proposal.engine, proposal.degraded, proposal.model)}</span>
        )}
      </div>

      <p className="ai-plan-hint">
        排期范围＝「{targetDate}」当天计划队列里未完成的条目；清单中没加进这一天队列的任务不受影响。
        选连续多天时，这些条目会分散排到之后几天。
      </p>

      <div className="ai-plan-options">
        <label className="ai-plan-field">
          <span>规划范围</span>
          <select
            className="text-input"
            disabled={busy}
            onChange={(event) => setHorizonDays(Number(event.target.value))}
            value={horizonDays}
          >
            {HORIZON_OPTIONS.map((days) => (
              <option key={days} value={days}>
                {days === 1 ? '仅当天' : `连续 ${days} 天`}
              </option>
            ))}
          </select>
        </label>

        <label className="ai-plan-check">
          <input
            checked={keepLockedBlocks}
            disabled={busy}
            onChange={(event) => setKeepLockedBlocks(event.target.checked)}
            type="checkbox"
          />
          <span>保留已锁定 / 手动安排的日程</span>
        </label>

        <label className="ai-plan-check">
          <input
            checked={respectPriority}
            disabled={busy}
            onChange={(event) => setRespectPriority(event.target.checked)}
            type="checkbox"
          />
          <span>优先按优先级排序</span>
        </label>

        <label className="ai-plan-field">
          <span>补充说明（可选，最多 200 字）</span>
          <textarea
            className="text-input ai-plan-instruction"
            disabled={busy}
            maxLength={200}
            onChange={(event) => setExtraInstruction(event.target.value)}
            placeholder="例如：上午先做数学，晚上留出背单词的时间"
            rows={2}
            value={extraInstruction}
          />
        </label>

        <button
          className="primary-action"
          disabled={busy}
          onClick={() => void handleGenerate()}
          type="button"
        >
          {loading ? <Loader2 className="spin" size={15} /> : <Sparkles size={15} />}
          {proposal ? '重新生成草案' : '生成草案'}
        </button>
      </div>

      {error && (
        <div className="ai-plan-alert is-error" role="alert">
          <AlertTriangle size={14} />
          <div>
            <strong>{error.message}</strong>
            {error.degraded && <small>{error.degraded.description}</small>}
            {error.retryable && (
              <button
                className="ghost-action ai-plan-retry"
                disabled={busy}
                onClick={() => void handleGenerate()}
                type="button"
              >
                <RefreshCw size={13} /> 重试
              </button>
            )}
            {!proposal && !allowLocalFallback && (
              <button
                className="ghost-action ai-plan-retry"
                disabled={busy}
                onClick={() => {
                  setAllowLocalFallback(true);
                  void handleGenerate();
                }}
                title="AI 暂时不可用时的替代方案：改用本地规则排期，结果会标明「本地兜底」"
                type="button"
              >
                <Sparkles size={13} /> 改用本地排期
              </button>
            )}
          </div>
        </div>
      )}

      {notice && !error && (
        <div className="ai-plan-alert is-info" role="status">
          <CheckCircle2 size={14} />
          <div>
            <strong>{notice}</strong>
          </div>
        </div>
      )}

      {applyResult && !error && applyResult.warnings.length > 0 && (
        <div className="ai-plan-alert is-warn">
          <AlertTriangle size={14} />
          <div>
            <strong>有 {applyResult.warnings.length} 条未能写入</strong>
            {applyResult.warnings.map((warning, index) => (
              <small key={`${warning.code}-${index}`}>{warning.message}</small>
            ))}
          </div>
        </div>
      )}

      <div className="ai-plan-body">
        {blockingWarnings.length > 0 && (
          <div className="ai-plan-alert is-warn">
            <AlertTriangle size={14} />
            <div>
              {blockingWarnings.map((warning, index) => (
                <small key={`${warning.code}-${index}`}>{warning.message}</small>
              ))}
            </div>
          </div>
        )}

        {proposal ? (
          <AiPlanTimeline
            categoryLabels={categoryLabels}
            items={proposal.items}
            showDateGroups={proposal.horizon_days > 1}
            unscheduled={proposal.unscheduled}
            warnings={proposal.warnings}
          />
        ) : (
          <div className="empty-state compact ai-plan-empty">
            <Sparkles size={24} />
            <strong>还没有草案</strong>
            <p>选好范围后点「生成草案」，先看看效果再决定要不要写进日历。</p>
          </div>
        )}
      </div>

      {proposal && (
        <div className="ai-plan-actions">
          <button
            className="ghost-action"
            disabled={busy}
            onClick={() => void handleDiscard()}
            type="button"
          >
            <Trash2 size={15} /> 放弃
          </button>
          <button
            className="primary-action"
            disabled={busy || proposal.items.length === 0}
            onClick={() => void handleApply()}
            type="button"
          >
            {applying ? <Loader2 className="spin" size={15} /> : <CheckCircle2 size={15} />}
            写入日历
          </button>
        </div>
      )}
    </section>
  );
}

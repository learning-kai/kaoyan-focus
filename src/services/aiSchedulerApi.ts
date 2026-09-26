/**
 * AI 智能日程规划的 Tauri 命令封装。
 *
 * 后端统一返回 `Result<T, String>`，其中 `Err` 是 JSON 字符串信封（见
 * `commands/ai_scheduler/models.rs` 的 `AiSchedulerError::to_envelope`）。
 * 这里统一解码成 `AiSchedulerError` 对象，避免上层拿到 `"[object Object]"`。
 */

import type {
  AiApplyOptions,
  AiApplyResult,
  AiConnectionTestResult,
  AiPlanItem,
  AiPlanProposal,
  AiPlanRequest,
  AiSchedulerError,
  AiSchedulerErrorCode,
  AiSchedulerSettings,
  ScheduleChangeEvent,
} from '../types/aiScheduler';
import { invokeCommand, isTauriRuntime, DESKTOP_RUNTIME_MESSAGE } from './tauriInvoke';

const FALLBACK_CODE: AiSchedulerErrorCode = 'network';

function toFallbackError(message: string, code: AiSchedulerErrorCode = FALLBACK_CODE): AiSchedulerError {
  return {
    code,
    message,
    retryable: false,
    retry_after_seconds: null,
    degraded: null,
  };
}

function isAiSchedulerError(value: unknown): value is AiSchedulerError {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const candidate = value as Partial<AiSchedulerError>;
  return typeof candidate.code === 'string' && typeof candidate.message === 'string';
}

/**
 * 把任意抛出物解析成 `AiSchedulerError`。
 *
 * 顺序很重要：先尝试 JSON 解密信封；不是信封时再判断是否运行在桌面端；
 * 最后兜底成 `network`，保证调用方永远拿到结构化错误。
 */
export function parseAiSchedulerError(raw: unknown): AiSchedulerError {
  if (isAiSchedulerError(raw)) {
    return {
      code: raw.code,
      message: raw.message,
      retryable: Boolean(raw.retryable),
      retry_after_seconds: raw.retry_after_seconds ?? null,
      degraded: raw.degraded ?? null,
    };
  }

  const text = raw instanceof Error ? raw.message : String(raw ?? '');

  // Rust 侧的 errors 是 JSON 字符串信封。
  const trimmed = text.trim();
  if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
    try {
      const decoded: unknown = JSON.parse(trimmed);
      if (isAiSchedulerError(decoded)) {
        return {
          code: decoded.code,
          message: decoded.message,
          retryable: Boolean(decoded.retryable),
          retry_after_seconds: decoded.retry_after_seconds ?? null,
          degraded: decoded.degraded ?? null,
        };
      }
    } catch {
      // 落到下面的兜底分支。
    }
  }

  if (!isTauriRuntime() || text.includes(DESKTOP_RUNTIME_MESSAGE)) {
    return toFallbackError(DESKTOP_RUNTIME_MESSAGE);
  }

  return toFallbackError(text || 'AI 排期请求失败，请稍后重试');
}

async function invokeAi<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invokeCommand<T>(command, args);
  } catch (raw) {
    throw parseAiSchedulerError(raw);
  }
}

/** 便于上层做类型收窄：`(reason as DesktopRuntimeUnavailableError)`。 */
export type { DesktopRuntimeUnavailableError } from './tauriInvoke';

export function getAiSchedulerSettings(): Promise<AiSchedulerSettings> {
  return invokeAi<AiSchedulerSettings>('get_ai_scheduler_settings');
}

export function saveAiSchedulerSettings(settings: AiSchedulerSettings): Promise<AiSchedulerSettings> {
  return invokeAi<AiSchedulerSettings>('save_ai_scheduler_settings', { settings });
}

export function testAiSchedulerConnection(): Promise<AiConnectionTestResult> {
  return invokeAi<AiConnectionTestResult>('test_ai_scheduler_connection');
}

export function previewAiSchedule(request: AiPlanRequest): Promise<AiPlanProposal> {
  return invokeAi<AiPlanProposal>('preview_ai_schedule', { request });
}

export function updateAiPlanProposalItems(
  proposalId: number,
  items: AiPlanItem[],
): Promise<AiPlanProposal> {
  return invokeAi<AiPlanProposal>('update_ai_plan_proposal_items', { proposalId, items });
}

export function regenerateAiPlanProposal(
  proposalId: number,
  feedback: string | null,
): Promise<AiPlanProposal> {
  return invokeAi<AiPlanProposal>('regenerate_ai_plan_proposal', { proposalId, feedback });
}

export function applyAiPlanProposal(
  proposalId: number,
  options: AiApplyOptions,
): Promise<AiApplyResult> {
  return invokeAi<AiApplyResult>('apply_ai_plan_proposal', { proposalId, options });
}

export function discardAiPlanProposal(proposalId: number): Promise<void> {
  return invokeAi<void>('discard_ai_plan_proposal', { proposalId });
}

export function getLatestAiPlanProposal(targetDate: string): Promise<AiPlanProposal | null> {
  return invokeAi<AiPlanProposal | null>('get_latest_ai_plan_proposal', { targetDate });
}

export function replanAiScheduleAfterChange(change: ScheduleChangeEvent): Promise<AiPlanProposal> {
  return invokeAi<AiPlanProposal>('replan_ai_schedule_after_change', { change });
}

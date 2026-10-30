/**
 * 正计时手动休息时填写的「休息理由」规则。
 *
 * 纯函数：不依赖 React / Tauri，供专注页、悬浮小组件与 Node 测试脚本共用。
 * 取值上限必须与 src-tauri/src/commands/focus/study_mode.rs 的 BREAK_REASON_MAX_CHARS 保持一致。
 */

/** 休息理由的最大字数，与后端校验一致。 */
export const BREAK_REASON_MAX_CHARS = 60;

export type BreakReasonValidation =
  | { ok: true; value: string; message: null }
  | { ok: false; value: string; message: string };

/** 去掉首尾空白后校验休息理由：必填，且不超过 BREAK_REASON_MAX_CHARS 个字符。 */
export function normalizeBreakReason(input: unknown): BreakReasonValidation {
  const trimmed = typeof input === 'string' ? input.trim() : '';
  if (!trimmed) {
    return { ok: false, value: '', message: '休息前请先写一句理由，说明为什么要休息。' };
  }
  if ([...trimmed].length > BREAK_REASON_MAX_CHARS) {
    return {
      ok: false,
      value: trimmed.slice(0, BREAK_REASON_MAX_CHARS),
      message: `休息理由最多 ${BREAK_REASON_MAX_CHARS} 字。`,
    };
  }
  return { ok: true, value: trimmed, message: null };
}

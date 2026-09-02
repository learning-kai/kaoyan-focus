/**
 * 番茄专注时长的取值规则与文案。
 *
 * 这里的函数都是纯函数：不依赖 React、不依赖 Tauri 运行时，
 * 因此可以被 UI、开始专注前的守卫以及 Node 测试脚本共用。
 */

/** 允许的番茄专注时长下限（分钟）。 */
export const FOCUS_MINUTES_MIN = 1;

/** 允许的番茄专注时长上限（分钟），与设置页 default_focus_minutes 的上限保持一致。 */
export const FOCUS_MINUTES_MAX = 120;

/** 没有历史偏好时回落的默认番茄时长（分钟）。 */
export const FOCUS_MINUTES_FALLBACK = 25;

/** 开始专注页提供的快捷预设（分钟），覆盖短冲刺到深度学习几种常见节奏。 */
export const FOCUS_PRESET_MINUTES = [15, 25, 45, 60, 90] as const;

export type FocusDurationValidation = {
  /** 是否可以立即用于开始专注。 */
  ok: boolean;
  /** 校验通过时为规范化后的分钟数，失败时为回落到合法范围内的值（不会是 NaN）。 */
  value: number;
  /** 失败原因，可直接展示给用户；校验通过时为 null。 */
  message: string | null;
};

/** 把任意输入收敛到合法区间内的整数分钟数，永不返回 NaN。 */
export function clampFocusMinutes(value: number): number {
  if (!Number.isFinite(value)) return FOCUS_MINUTES_FALLBACK;
  const floored = Math.floor(value);
  if (floored < FOCUS_MINUTES_MIN) return FOCUS_MINUTES_MIN;
  if (floored > FOCUS_MINUTES_MAX) return FOCUS_MINUTES_MAX;
  return floored;
}

/** 判断某个分钟数是否是快捷预设之一。 */
export function isFocusPresetMinutes(value: number): boolean {
  return FOCUS_PRESET_MINUTES.some((preset) => preset === value);
}

/**
 * 严格解析输入：只有落在有效区间内的整数分钟才会被接受。
 * 其余情况（空值、非数字、小数、越界）一律返回 null，由调用方决定是否保留旧值。
 */
export function parseFocusMinutes(raw: string | number | null | undefined): number | null {
  if (raw === null || raw === undefined) return null;
  const text = typeof raw === 'number' ? String(raw) : raw.trim();
  if (text === '') return null;
  if (!/^\d+$/.test(text)) return null;
  const parsed = Number(text);
  if (!Number.isInteger(parsed)) return null;
  if (parsed < FOCUS_MINUTES_MIN || parsed > FOCUS_MINUTES_MAX) return null;
  return parsed;
}

/**
 * 校验番茄专注时长，返回可直接渲染的结果。
 *
 * 规则：
 * 1. 必填，不能为空；
 * 2. 必须是整数分钟，不接受小数或其它字符；
 * 3. 必须落在 1-120 分钟之间。
 */
export function validateFocusMinutes(raw: string | number | null | undefined): FocusDurationValidation {
  if (raw === null || raw === undefined) {
    return { ok: false, value: FOCUS_MINUTES_FALLBACK, message: '请先填写番茄专注时长。' };
  }

  const text = typeof raw === 'number' ? String(raw) : raw.trim();
  if (text === '') {
    return { ok: false, value: FOCUS_MINUTES_FALLBACK, message: '请先填写番茄专注时长。' };
  }

  if (!/^\d+$/.test(text)) {
    return {
      ok: false,
      value: FOCUS_MINUTES_FALLBACK,
      message: `番茄专注时长需要是 ${FOCUS_MINUTES_MIN}-${FOCUS_MINUTES_MAX} 之间的整数分钟。`,
    };
  }

  const parsed = Number(text);
  if (parsed < FOCUS_MINUTES_MIN || parsed > FOCUS_MINUTES_MAX) {
    return {
      ok: false,
      value: clampFocusMinutes(parsed),
      message: `番茄专注时长需要在 ${FOCUS_MINUTES_MIN}-${FOCUS_MINUTES_MAX} 分钟之间。`,
    };
  }

  return { ok: true, value: parsed, message: null };
}

/** 把分钟数格式化为可读时长，用于按钮、摘要和通知文案。 */
export function formatFocusDurationLabel(minutes: number): string {
  const safeMinutes = clampFocusMinutes(minutes);
  if (safeMinutes < 60) return safeMinutes + ' 分钟';
  const hours = Math.floor(safeMinutes / 60);
  const restMinutes = safeMinutes % 60;
  return restMinutes === 0 ? hours + ' 小时' : hours + ' 小时 ' + restMinutes + ' 分钟';
}

/** 把分钟数格式化为 mm:ss / h:mm:ss 形式的倒计时初值。 */
export function formatFocusCountdown(minutes: number): string {
  const totalSeconds = clampFocusMinutes(minutes) * 60;
  const hours = Math.floor(totalSeconds / 3600);
  const minutePart = Math.floor((totalSeconds % 3600) / 60)
    .toString()
    .padStart(2, '0');
  const secondPart = Math.floor(totalSeconds % 60)
    .toString()
    .padStart(2, '0');
  return hours > 0 ? hours + ':' + minutePart + ':' + secondPart : minutePart + ':' + secondPart;
}

/**
 * 随机微休息的设置规则与预设。
 *
 * 纯函数：不依赖 React / Tauri，供面板 UI、协调器和 Node 测试脚本共用。
 * 取值范围必须与 src-tauri/src/commands/micro_break.rs 的 normalized() 保持一致。
 */

export type MicroBreakSettings = {
  enabled: boolean;
  min_interval_seconds: number;
  max_interval_seconds: number;
  rest_seconds: number;
  /** 按专注算法随时长调整提示密度。 */
  adaptive: boolean;
  /** 提示音音量 0-100。 */
  volume: number;
};

/** 后端事件 `study-micro-break-cue` 的载荷。 */
export type MicroBreakCue = {
  study_mode_id: number;
  cycle_index: number;
  cue_number: number;
  rest_seconds: number;
  volume: number;
  phase_elapsed_seconds: number;
};

export const MICRO_BREAK_CUE_EVENT = 'study-micro-break-cue';

export const MICRO_BREAK_MIN_INTERVAL_SECONDS = 60;
export const MICRO_BREAK_MAX_INTERVAL_SECONDS = 30 * 60;
export const MICRO_BREAK_REST_MIN_SECONDS = 3;
export const MICRO_BREAK_REST_MAX_SECONDS = 60;

export const DEFAULT_MICRO_BREAK_SETTINGS: MicroBreakSettings = {
  enabled: false,
  min_interval_seconds: 3 * 60,
  max_interval_seconds: 5 * 60,
  rest_seconds: 10,
  adaptive: true,
  volume: 60,
};

export type MicroBreakIntervalPreset = {
  id: string;
  label: string;
  min_interval_seconds: number;
  max_interval_seconds: number;
};

export const MICRO_BREAK_INTERVAL_PRESETS: readonly MicroBreakIntervalPreset[] = [
  { id: '2-4', label: '2–4 分钟', min_interval_seconds: 2 * 60, max_interval_seconds: 4 * 60 },
  { id: '3-5', label: '3–5 分钟', min_interval_seconds: 3 * 60, max_interval_seconds: 5 * 60 },
  { id: '5-8', label: '5–8 分钟', min_interval_seconds: 5 * 60, max_interval_seconds: 8 * 60 },
];

export const MICRO_BREAK_REST_PRESETS: readonly number[] = [5, 10, 15, 20];

/** 「90 分钟专注 + 20 分钟休息」循环的一键预设。 */
export const MICRO_BREAK_METHOD_PRESET = {
  focus_minutes: 90,
  break_minutes: 20,
  long_break_minutes: 20,
  /** 两轮 90 分钟。学习模式总时长只统计专注时间，不含休息。 */
  study_minutes: 180,
} as const;

function clampInteger(value: unknown, min: number, max: number, fallback: number): number {
  const numeric = typeof value === 'number' ? value : Number.NaN;
  if (!Number.isFinite(numeric)) return fallback;
  return Math.min(max, Math.max(min, Math.round(numeric)));
}

/** 与后端 normalized() 同一套规则：钳制范围，区间上下限颠倒时交换。 */
export function normalizeMicroBreakSettings(input: Partial<MicroBreakSettings> | null | undefined): MicroBreakSettings {
  const source = input ?? {};
  const defaults = DEFAULT_MICRO_BREAK_SETTINGS;
  let min = clampInteger(source.min_interval_seconds, MICRO_BREAK_MIN_INTERVAL_SECONDS, MICRO_BREAK_MAX_INTERVAL_SECONDS, defaults.min_interval_seconds);
  let max = clampInteger(source.max_interval_seconds, MICRO_BREAK_MIN_INTERVAL_SECONDS, MICRO_BREAK_MAX_INTERVAL_SECONDS, defaults.max_interval_seconds);
  if (min > max) [min, max] = [max, min];
  return {
    enabled: source.enabled === true,
    min_interval_seconds: min,
    max_interval_seconds: max,
    rest_seconds: clampInteger(source.rest_seconds, MICRO_BREAK_REST_MIN_SECONDS, MICRO_BREAK_REST_MAX_SECONDS, defaults.rest_seconds),
    adaptive: source.adaptive === undefined ? defaults.adaptive : source.adaptive === true,
    volume: clampInteger(source.volume, 0, 100, defaults.volume),
  };
}

export function findIntervalPreset(settings: MicroBreakSettings): MicroBreakIntervalPreset | undefined {
  return MICRO_BREAK_INTERVAL_PRESETS.find(
    (preset) => preset.min_interval_seconds === settings.min_interval_seconds && preset.max_interval_seconds === settings.max_interval_seconds,
  );
}

function formatMinutes(seconds: number): string {
  const minutes = seconds / 60;
  return Number.isInteger(minutes) ? String(minutes) : minutes.toFixed(1);
}

/** 例如「每 3–5 分钟随机提示，闭眼 10 秒」。 */
export function describeMicroBreak(settings: MicroBreakSettings): string {
  if (!settings.enabled) return '未开启';
  const range =
    settings.min_interval_seconds === settings.max_interval_seconds
      ? `每 ${formatMinutes(settings.min_interval_seconds)} 分钟`
      : `每 ${formatMinutes(settings.min_interval_seconds)}–${formatMinutes(settings.max_interval_seconds)} 分钟随机`;
  return `${range}提示，闭眼 ${settings.rest_seconds} 秒`;
}

/** 闭眼倒计时剩余整秒数（向上取整，最后一秒显示 1 而不是 0）。 */
export function microBreakSecondsLeft(endsAt: number, now: number): number {
  return Math.max(0, Math.ceil((endsAt - now) / 1000));
}

/**
 * 全站配色的 TypeScript 侧唯一真源。
 *
 * 这些色值必须与下面两处保持一致，否则同一科目会出现多套颜色：
 * 1. `src/styles.css` 的 `--subject-*` / `--focus-band-pause` 变量；
 * 2. `src-tauri/src/storage/db.rs` 的 `DEFAULT_SUBJECTS`（科目种子色，每次启动都会回写）。
 *
 * 选色刻意避开 `--green`（休息 #34c759）与 `--amber`（等待休息 #ff9500）所在的色域，
 * 避免「专注 / 休息 / 暂停」三类区间在日历时间轴上互相混淆。
 */
export const SUBJECT_PALETTE = {
  /** 政治：红（与危险红 #ff3b30 拉开 ΔE≈23） */
  politics: '#e5484d',
  /** 英语：天蓝（与品牌蓝 #007aff 拉开 ΔE≈46） */
  english: '#0ea5e9',
  /** 数学：深琥珀（刻意不使用绿色，与休息绿 #34c759 拉开 ΔE≈100） */
  math: '#b45309',
  /** 专业课：紫 */
  major: '#a855f7',
  /** 通用 / 未分类：浅石板灰 */
  general: '#94a3b8',
} as const;

/** 无科目会话的专注色带兜底色：中性石板灰，不再使用容易被误读成「休息」的薄荷绿。 */
export const FOCUS_BAND_FALLBACK_COLOR = SUBJECT_PALETTE.general;

/** 暂停区间的色带颜色：比通用灰更深（ΔE≈22），且与科目色、休息绿完全区分。 */
export const PAUSE_BAND_COLOR = '#5f6b7f';

export type SubjectPalette = typeof SUBJECT_PALETTE;
export type SubjectPaletteKey = keyof SubjectPalette;

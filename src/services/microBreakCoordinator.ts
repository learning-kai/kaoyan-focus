/**
 * 随机微休息的前端协调器（主窗口模块级单例）。
 *
 * 提示时刻由 Rust 后台 tick 判定并发出 `study-micro-break-cue` 事件，这里只负责：
 * 1. 播放「闭眼」提示音；
 * 2. 维持 rest_seconds 秒的闭眼状态供界面显示倒计时；
 * 3. 到时播放「睁眼」提示音。
 *
 * 结束计时用一次性的 setTimeout，并以绝对时间 endsAt 为准：
 * 即使窗口隐藏导致定时器略有延迟，倒计时显示也不会漂移。
 */
import { getMicroBreakSettings, saveMicroBreakSettings } from './microBreakApi';
import { listenTauriEvent } from './tauriEvents';
import { DesktopRuntimeUnavailableError } from './tauriInvoke';
import {
  DEFAULT_MICRO_BREAK_SETTINGS,
  MICRO_BREAK_CUE_EVENT,
  normalizeMicroBreakSettings,
  type MicroBreakCue,
  type MicroBreakSettings,
} from '../utils/microBreak';

export type MicroBreakSettingsStatus = 'idle' | 'loading' | 'ready' | 'unavailable' | 'error';

export type MicroBreakSnapshot = {
  settings: MicroBreakSettings;
  /** unavailable = 浏览器预览，改动只在本页生效、不落盘。 */
  settingsStatus: MicroBreakSettingsStatus;
  settingsError: string | null;
  /** 正在闭眼休息时的状态；为 null 表示当前没有微休息。 */
  active: { cueNumber: number; restSeconds: number; endsAt: number; preview: boolean } | null;
  /** 最近一次提示，用于在专注界面显示「已微休息 N 次」。 */
  lastCue: { studyModeId: number; cycleIndex: number; cueNumber: number } | null;
};

type Tone = { offset: number; frequency: number; length: number; gain: number };

/** 闭眼：柔和的下行两音。 */
const START_TONES: Tone[] = [
  { offset: 0, frequency: 784, length: 0.5, gain: 0.16 },
  { offset: 0.32, frequency: 587, length: 0.7, gain: 0.14 },
];

/** 睁眼：轻快的上行三音，与闭眼提示区分开。 */
const END_TONES: Tone[] = [
  { offset: 0, frequency: 587, length: 0.22, gain: 0.13 },
  { offset: 0.18, frequency: 784, length: 0.22, gain: 0.14 },
  { offset: 0.36, frequency: 1046, length: 0.42, gain: 0.15 },
];

const SAVE_DEBOUNCE_MS = 300;

const listeners = new Set<() => void>();
let snapshot: MicroBreakSnapshot = {
  settings: DEFAULT_MICRO_BREAK_SETTINGS,
  settingsStatus: 'idle',
  settingsError: null,
  active: null,
  lastCue: null,
};
let endTimer: number | null = null;
let saveTimer: number | null = null;
let audioContext: AudioContext | null = null;

function emit(next: MicroBreakSnapshot) {
  snapshot = next;
  for (const listener of listeners) listener();
}

function playTones(tones: Tone[], volume: number) {
  const ratio = Math.min(1, Math.max(0, volume / 100));
  if (ratio <= 0) return;
  try {
    const AudioContextClass = window.AudioContext || (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextClass) return;
    audioContext ??= new AudioContextClass();
    if (audioContext.state === 'suspended') void audioContext.resume();
    const now = audioContext.currentTime;
    for (const tone of tones) {
      const oscillator = audioContext.createOscillator();
      const gain = audioContext.createGain();
      const startAt = now + tone.offset;
      const endAt = startAt + tone.length;
      oscillator.type = 'sine';
      oscillator.frequency.setValueAtTime(tone.frequency, startAt);
      gain.gain.setValueAtTime(0.0001, startAt);
      gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, tone.gain * ratio), startAt + 0.04);
      gain.gain.exponentialRampToValueAtTime(0.0001, endAt);
      oscillator.connect(gain);
      gain.connect(audioContext.destination);
      oscillator.start(startAt);
      oscillator.stop(endAt + 0.02);
    }
  } catch {
    // 提示音是尽力而为，界面倒计时仍然会显示。
  }
}

function clearEndTimer() {
  if (endTimer !== null) {
    window.clearTimeout(endTimer);
    endTimer = null;
  }
}

function beginRest(cueNumber: number, restSeconds: number, volume: number, preview: boolean) {
  clearEndTimer();
  const endsAt = Date.now() + restSeconds * 1000;
  playTones(START_TONES, volume);
  emit({ ...snapshot, active: { cueNumber, restSeconds, endsAt, preview } });
  endTimer = window.setTimeout(() => {
    endTimer = null;
    playTones(END_TONES, volume);
    emit({ ...snapshot, active: null });
  }, restSeconds * 1000);
}

function handleCue(cue: MicroBreakCue) {
  // 上一次闭眼还没结束时不叠加（提示间隔至少 1 分钟，正常不会发生）。
  if (snapshot.active && !snapshot.active.preview) return;
  snapshot = { ...snapshot, lastCue: { studyModeId: cue.study_mode_id, cycleIndex: cue.cycle_index, cueNumber: cue.cue_number } };
  beginRest(cue.cue_number, cue.rest_seconds, cue.volume, false);
}

/** 用户主动提前结束：已经睁眼了，不再播放睁眼提示音。 */
export function endMicroBreakEarly() {
  if (!snapshot.active) return;
  clearEndTimer();
  emit({ ...snapshot, active: null });
}

/** 设置面板里的「试听」：完整走一遍闭眼 → 睁眼，浏览器预览里也可用。 */
export function previewMicroBreak(restSeconds: number, volume: number) {
  beginRest(0, restSeconds, volume, true);
}

function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/** 读取设置；已经读过时直接返回，除非 force。 */
export async function loadMicroBreakSettings(force = false): Promise<void> {
  if (!force && (snapshot.settingsStatus === 'loading' || snapshot.settingsStatus === 'ready')) return;
  emit({ ...snapshot, settingsStatus: 'loading', settingsError: null });
  try {
    const settings = await getMicroBreakSettings();
    emit({ ...snapshot, settings, settingsStatus: 'ready', settingsError: null });
  } catch (reason) {
    if (reason instanceof DesktopRuntimeUnavailableError) {
      emit({ ...snapshot, settingsStatus: 'unavailable', settingsError: null });
    } else {
      emit({ ...snapshot, settingsStatus: 'error', settingsError: errorMessage(reason) });
    }
  }
}

/**
 * 修改设置：界面立即生效，300ms 防抖后落盘（拖动音量滑块时不会连续写库）。
 * 后端在下一次 tick（≤3 秒）读取新设置。
 */
export function updateMicroBreakSettings(patch: Partial<MicroBreakSettings>) {
  const settings = normalizeMicroBreakSettings({ ...snapshot.settings, ...patch });
  emit({ ...snapshot, settings, settingsError: null });
  if (snapshot.settingsStatus === 'unavailable') return;

  if (saveTimer !== null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = null;
    const toSave = snapshot.settings;
    saveMicroBreakSettings(toSave)
      .then((saved) => {
        // 保存期间用户又改了，就以更新的本地值为准，等下一次保存。
        if (snapshot.settings === toSave) emit({ ...snapshot, settings: saved, settingsStatus: 'ready' });
      })
      .catch((reason) => {
        emit({ ...snapshot, settingsError: '微休息设置保存失败：' + errorMessage(reason) });
      });
  }, SAVE_DEBOUNCE_MS);
}

export function getMicroBreakSnapshot(): MicroBreakSnapshot {
  return snapshot;
}

export function subscribeMicroBreak(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** 在主窗口挂载一次，开始监听后端的微休息提示事件。返回取消监听函数。 */
export function startMicroBreakListener(): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void listenTauriEvent<MicroBreakCue>(MICRO_BREAK_CUE_EVENT, (event) => handleCue(event.payload))
    .then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    })
    .catch(() => {
      // 浏览器预览没有桌面事件。
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}

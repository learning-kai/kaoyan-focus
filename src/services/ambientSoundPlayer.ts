/**
 * 白噪音混音播放器（主窗口单例）。
 *
 * - 每个音源一个循环播放的 HTMLAudioElement，按需创建；
 *   不用 Web Audio 解码成 PCM，是为了避免十几分钟的音源在内存里展开成上百 MB。
 * - 状态放在模块级，页面切换不会打断播放；React 通过 subscribe/getSnapshot 订阅。
 * - 纯规则（音量换算、存储格式）在 utils/ambientSound.ts，这里只负责副作用。
 * - 只在主窗口使用，专注悬浮窗不引入本模块。
 */
import {
  ambientSoundUrl,
  buildAmbientPlaybackPlan,
  clearAmbientMix,
  findAmbientSound,
  parseStoredAmbientMix,
  serializeAmbientMix,
  setAmbientEnabled,
  setAmbientMasterVolume,
  setAmbientSoundVolume,
  toggleAmbientSound,
  type AmbientMixState,
} from '../utils/ambientSound';

export const AMBIENT_MIX_STORAGE_KEY = 'kaoyan-focus-ambient-mix';

/** 开始 / 停止时的淡入淡出时长，避免爆音。 */
const FADE_TOGGLE_MS = 400;
/** 拖动音量时的平滑时长，短到跟手即可。 */
const FADE_ADJUST_MS = 80;
/** 用定时器而不是 rAF：窗口最小化到托盘后 rAF 会停，淡出就永远不会结束。 */
const FADE_STEP_MS = 20;

export type AmbientSoundSnapshot = {
  mix: AmbientMixState;
  /** 加载或播放失败的音源 id。 */
  failedIds: readonly string[];
};

type Channel = {
  audio: HTMLAudioElement;
  fadeTimer: number | null;
  /** 最近一次 sync 给出的目标增益；0 表示应当停下。 */
  targetGain: number;
};

const channels = new Map<string, Channel>();
const listeners = new Set<() => void>();
let failedIds = new Set<string>();
let mix: AmbientMixState = loadMix();
let snapshot: AmbientSoundSnapshot = { mix, failedIds: [] };

function loadMix(): AmbientMixState {
  try {
    return parseStoredAmbientMix(window.localStorage.getItem(AMBIENT_MIX_STORAGE_KEY));
  } catch {
    return parseStoredAmbientMix(null);
  }
}

function persistMix() {
  try {
    window.localStorage.setItem(AMBIENT_MIX_STORAGE_KEY, serializeAmbientMix(mix));
  } catch {
    // 存储不可用时只影响下次启动的恢复，不影响当前播放。
  }
}

function emit() {
  snapshot = { mix, failedIds: Array.from(failedIds) };
  for (const listener of listeners) listener();
}

function markFailed(id: string, failed: boolean) {
  if (failedIds.has(id) === failed) return;
  const next = new Set(failedIds);
  if (failed) next.add(id);
  else next.delete(id);
  failedIds = next;
  emit();
}

function clearFade(channel: Channel) {
  if (channel.fadeTimer !== null) {
    window.clearInterval(channel.fadeTimer);
    channel.fadeTimer = null;
  }
}

function fadeTo(channel: Channel, target: number, durationMs: number, onDone?: () => void) {
  clearFade(channel);
  const start = channel.audio.volume;
  const steps = Math.max(1, Math.round(durationMs / FADE_STEP_MS));
  if (steps <= 1 || Math.abs(target - start) < 0.005) {
    channel.audio.volume = target;
    onDone?.();
    return;
  }
  let step = 0;
  channel.fadeTimer = window.setInterval(() => {
    step += 1;
    const ratio = Math.min(1, step / steps);
    channel.audio.volume = Math.min(1, Math.max(0, start + (target - start) * ratio));
    if (ratio >= 1) {
      clearFade(channel);
      onDone?.();
    }
  }, FADE_STEP_MS);
}

function disposeChannel(id: string) {
  const channel = channels.get(id);
  if (!channel) return;
  clearFade(channel);
  channel.audio.pause();
  channel.audio.removeAttribute('src');
  channel.audio.load();
  channels.delete(id);
}

function ensureChannel(id: string): Channel | null {
  const existing = channels.get(id);
  if (existing) return existing;

  const sound = findAmbientSound(id);
  if (!sound) return null;

  const audio = new Audio(ambientSoundUrl(sound.file));
  audio.loop = true;
  audio.preload = 'auto';
  audio.volume = 0;
  const channel: Channel = { audio, fadeTimer: null, targetGain: 0 };
  audio.addEventListener('error', () => {
    // 丢掉坏掉的元素，下次打开会重新创建并重试。
    disposeChannel(id);
    markFailed(id, true);
  });
  audio.addEventListener('playing', () => markFailed(id, false));
  channels.set(id, channel);
  return channel;
}

function startChannel(id: string, channel: Channel, gain: number) {
  const wasPaused = channel.audio.paused;
  channel.targetGain = gain;
  if (wasPaused) {
    channel.audio.volume = 0;
    channel.audio.play().catch((error: unknown) => {
      // 被新的 pause() 打断属于正常流程，不算失败。
      if (error instanceof DOMException && error.name === 'AbortError') return;
      if (channels.get(id) !== channel || channel.targetGain <= 0) return;
      markFailed(id, true);
    });
  }
  fadeTo(channel, gain, wasPaused ? FADE_TOGGLE_MS : FADE_ADJUST_MS);
}

function stopChannel(channel: Channel) {
  if (channel.targetGain <= 0 && channel.audio.paused) return;
  channel.targetGain = 0;
  fadeTo(channel, 0, FADE_TOGGLE_MS, () => {
    // 淡出期间如果又被重新打开，就不要暂停。
    if (channel.targetGain <= 0) channel.audio.pause();
  });
}

function syncPlayback() {
  const plan = buildAmbientPlaybackPlan(mix);
  const planned = new Set<string>();

  for (const item of plan) {
    planned.add(item.id);
    const channel = ensureChannel(item.id);
    if (channel) startChannel(item.id, channel, item.gain);
  }

  for (const [id, channel] of channels) {
    if (!planned.has(id)) stopChannel(channel);
  }
}

function update(next: AmbientMixState) {
  if (next === mix) return;
  mix = next;
  persistMix();
  emit();
  syncPlayback();
}

export function getAmbientSoundSnapshot(): AmbientSoundSnapshot {
  return snapshot;
}

export function subscribeAmbientSound(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function toggleAmbientSoundChannel(id: string) {
  // 用户主动重试时清掉失败标记，让界面先回到正常态。
  if (failedIds.has(id)) markFailed(id, false);
  update(toggleAmbientSound(mix, id));
}

export function setAmbientSoundChannelVolume(id: string, volume: number) {
  update(setAmbientSoundVolume(mix, id, volume));
}

export function setAmbientSoundMasterVolume(volume: number) {
  update(setAmbientMasterVolume(mix, volume));
}

export function setAmbientSoundEnabled(enabled: boolean) {
  update(setAmbientEnabled(mix, enabled));
}

export function clearAmbientSoundMix() {
  update(clearAmbientMix(mix));
}

import { getAppSettings, getCustomReminderSound } from './settingsApi';
import { showStudyReminder } from './systemApi';
import type { AppSettings, ReminderSoundId, ReminderSoundSource } from '../types/settings';

type ReminderPayload = {
  title: string;
  body: string;
  wakeWindow?: boolean;
};

type SoundStop = () => void;

type ReminderSoundSettings = Pick<
  AppSettings,
  | 'reminder_sound_source'
  | 'reminder_sound_id'
  | 'reminder_sound_file_name'
  | 'reminder_sound_updated_at'
  | 'reminder_sound_volume'
  | 'reminder_sound_duration_seconds'
  | 'reminder_quiet_hours_enabled'
  | 'reminder_quiet_hours_start'
  | 'reminder_quiet_hours_end'
>;

type BuiltInNote = {
  offset: number;
  frequency: number;
  length: number;
  type: OscillatorType;
  gain: number;
};

type BuiltInPreset = {
  notes: BuiltInNote[];
  repeatMs: number;
};

const defaultReminderSoundSettings: ReminderSoundSettings = {
  reminder_sound_source: 'builtin',
  reminder_sound_id: 'classic',
  reminder_sound_file_name: null,
  reminder_sound_updated_at: null,
  reminder_sound_volume: 100,
  reminder_sound_duration_seconds: 30,
  reminder_quiet_hours_enabled: false,
  reminder_quiet_hours_start: '22:30',
  reminder_quiet_hours_end: '07:00',
};

const builtInSoundPresets: Record<ReminderSoundId, BuiltInPreset> = {
  classic: {
    repeatMs: 2200,
    notes: [
      { offset: 0, frequency: 880, length: 0.28, type: 'sine', gain: 0.2 },
      { offset: 0.34, frequency: 1174, length: 0.3, type: 'triangle', gain: 0.22 },
      { offset: 0.74, frequency: 988, length: 0.34, type: 'sine', gain: 0.18 },
      { offset: 1.18, frequency: 1318, length: 0.4, type: 'triangle', gain: 0.24 },
    ],
  },
  bright: {
    repeatMs: 1800,
    notes: [
      { offset: 0, frequency: 1046, length: 0.18, type: 'triangle', gain: 0.22 },
      { offset: 0.22, frequency: 1318, length: 0.2, type: 'triangle', gain: 0.24 },
      { offset: 0.48, frequency: 1568, length: 0.26, type: 'sine', gain: 0.22 },
    ],
  },
  soft: {
    repeatMs: 2600,
    notes: [
      { offset: 0, frequency: 523, length: 0.42, type: 'sine', gain: 0.12 },
      { offset: 0.5, frequency: 659, length: 0.46, type: 'sine', gain: 0.14 },
      { offset: 1.05, frequency: 784, length: 0.5, type: 'triangle', gain: 0.12 },
    ],
  },
  urgent: {
    repeatMs: 1500,
    notes: [
      { offset: 0, frequency: 880, length: 0.16, type: 'square', gain: 0.2 },
      { offset: 0.22, frequency: 880, length: 0.16, type: 'square', gain: 0.2 },
      { offset: 0.44, frequency: 1174, length: 0.24, type: 'sawtooth', gain: 0.18 },
      { offset: 0.78, frequency: 1174, length: 0.28, type: 'sawtooth', gain: 0.18 },
    ],
  },
  short: {
    repeatMs: 1200,
    notes: [
      { offset: 0, frequency: 988, length: 0.16, type: 'sine', gain: 0.18 },
      { offset: 0.2, frequency: 1318, length: 0.18, type: 'triangle', gain: 0.18 },
    ],
  },
};

const REMINDER_NOTIFICATION_CLOSED_EVENT = 'study-reminder-notification-closed';

let audioContext: AudioContext | null = null;
let customSoundCache: { key: string; url: string } | null = null;
let previewSoundStop: SoundStop | null = null;
let previewNotificationId: string | null = null;
let previewObjectUrl: string | null = null;
let notificationSequence = 0;
let notificationClosedListenerStarted = false;
const persistentAlarmSounds = new Map<string, () => void>();
const activeNotificationSounds = new Map<string, { stop: SoundStop; timeoutId: number | null }>();

export async function notifyStudyReminder(payload: ReminderPayload) {
  const settings = await loadReminderSoundSettings();
  const notificationId = createNotificationId('reminder');
  const stopSound = isQuietHoursActive(settings) ? null : await playReminderSound(settings);
  if (stopSound) {
    const durationMs = settings.reminder_sound_duration_seconds * 1000;
    registerNotificationSound(notificationId, stopSound, durationMs);
  }
  await showDesktopNotification(payload, settings, notificationId);
}

export async function notifyPersistentAlarm(key: string, payload: ReminderPayload) {
  const settings = await loadReminderSoundSettings();
  if (!isQuietHoursActive(settings) && !persistentAlarmSounds.has(key)) {
    await startPersistentAlarmSound(key);
  }

  if (persistentAlarmSounds.has(key)) {
    const durationMs = settings.reminder_sound_duration_seconds * 1000;
    registerNotificationSound(key, () => stopPersistentAlarmAudio(key), durationMs);
  }
  await showDesktopNotification(payload, settings, key);
}

export function stopPersistentAlarmSound(key?: string) {
  if (key) {
    stopPersistentAlarmAudio(key);
    stopNotificationSound(key);
    return;
  }

  for (const alarmKey of Array.from(persistentAlarmSounds.keys())) {
    stopPersistentAlarmAudio(alarmKey);
  }
  for (const notificationId of Array.from(activeNotificationSounds.keys())) {
    stopNotificationSound(notificationId);
  }
}

export async function previewReminderSound(settings?: Partial<ReminderSoundSettings>, customFile?: File) {
  stopPreviewAudio();
  const normalizedSettings = normalizeReminderSoundSettings(settings);
  const stopSound = await playReminderSound(normalizedSettings, customFile, true);
  if (!stopSound) {
    return;
  }

  const notificationId = createNotificationId('preview');
  previewSoundStop = stopSound;
  previewNotificationId = notificationId;
  const durationMs = normalizedSettings.reminder_sound_duration_seconds * 1000;
  registerNotificationSound(notificationId, () => stopPreviewAudio(), durationMs);
  await showDesktopNotification(
    {
      title: '正在试听提醒铃声',
      body: '关闭这条通知即可停止试听音乐。',
    },
    silentNotificationSettings(normalizedSettings),
    notificationId,
  );
}

async function startPersistentAlarmSound(key: string) {
  const settings = await loadReminderSoundSettings();

  if (settings.reminder_sound_source === 'custom') {
    const audio = await playStoredCustomSound(true, settings.reminder_sound_updated_at, settings.reminder_sound_volume);
    if (audio) {
      persistentAlarmSounds.set(key, audio);
      return;
    }
  }

  const soundId = normalizeReminderSoundId(settings.reminder_sound_id);
  let stopCurrentSound = playBuiltInSound(soundId, settings.reminder_sound_volume);
  const timerId = window.setInterval(() => {
    stopCurrentSound?.();
    stopCurrentSound = playBuiltInSound(soundId, settings.reminder_sound_volume);
  }, builtInSoundPresets[soundId].repeatMs);
  persistentAlarmSounds.set(key, () => {
    window.clearInterval(timerId);
    stopCurrentSound?.();
  });
}

async function playReminderSound(settings: ReminderSoundSettings, customFile?: File, strictCustom = false): Promise<SoundStop | null> {
  if (settings.reminder_sound_source === 'custom') {
    if (customFile) {
      const stopSound = await playCustomFile(customFile, settings.reminder_sound_volume);
      if (stopSound) {
        return stopSound;
      }
    } else {
      const stopSound = await playStoredCustomSound(false, settings.reminder_sound_updated_at, settings.reminder_sound_volume);
      if (stopSound) {
        return stopSound;
      }
    }

    if (strictCustom) {
      throw new Error('自定义音频播放失败，请换一个 mp3、wav、ogg 或 m4a 文件试试。');
    }
  }

  return playBuiltInSound(settings.reminder_sound_id, settings.reminder_sound_volume);
}

function playBuiltInSound(soundId: ReminderSoundId, volume: number): SoundStop | null {
  try {
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (!AudioContextClass) {
      return null;
    }

    audioContext ??= new AudioContextClass();
    if (audioContext.state === 'suspended') {
      void audioContext.resume();
    }

    const now = audioContext.currentTime;
    const volumeRatio = volumeToRatio(volume);
    if (volumeRatio <= 0) {
      return () => {};
    }
    const oscillators: OscillatorNode[] = [];
    for (const note of builtInSoundPresets[soundId].notes) {
      const oscillator = audioContext.createOscillator();
      const gain = audioContext.createGain();
      const startAt = now + note.offset;
      const peakAt = startAt + 0.035;
      const endAt = startAt + note.length;

      oscillator.type = note.type;
      oscillator.frequency.setValueAtTime(note.frequency, startAt);
      oscillator.frequency.exponentialRampToValueAtTime(note.frequency * 1.04, endAt);

      gain.gain.setValueAtTime(0.0001, startAt);
      gain.gain.exponentialRampToValueAtTime(Math.max(0.0001, note.gain * volumeRatio), peakAt);
      gain.gain.exponentialRampToValueAtTime(0.0001, endAt);

      oscillator.connect(gain);
      gain.connect(audioContext.destination);
      oscillator.start(startAt);
      oscillator.stop(endAt + 0.02);
      oscillators.push(oscillator);
    }

    return () => {
      for (const oscillator of oscillators) {
        try {
          oscillator.stop();
        } catch {
          // Oscillator may have already stopped naturally.
        }
      }
    };
  } catch {
    // Sound reminders are best-effort. Notification still handles the visible cue.
  }
  return null;
}

async function playStoredCustomSound(loop: boolean, updatedAt: number | null, volume: number) {
  try {
    const url = await getCustomSoundUrl(updatedAt);
    if (!url) {
      return null;
    }
    return playAudioUrl(url, loop, false, volume);
  } catch {
    return null;
  }
}

async function playCustomFile(file: File, volume: number) {
  const url = URL.createObjectURL(file);
  previewObjectUrl = url;
  return playAudioUrl(url, false, true, volume);
}

async function playAudioUrl(url: string, loop: boolean, revokeOnEnd: boolean, volume: number) {
  const audio = new Audio(url);
  audio.loop = loop;
  audio.volume = 0.9 * volumeToRatio(volume);
  let stopped = false;
  const cleanup = () => {
    if (revokeOnEnd && previewObjectUrl === url) {
      URL.revokeObjectURL(url);
      previewObjectUrl = null;
    }
  };
  const stopSound = () => {
    if (stopped) {
      return;
    }
    stopped = true;
    audio.pause();
    audio.currentTime = 0;
    audio.removeAttribute('src');
    audio.load();
    cleanup();
  };
  audio.addEventListener('ended', cleanup, { once: true });
  audio.addEventListener('error', cleanup, { once: true });

  try {
    await audio.play();
    return stopSound;
  } catch {
    cleanup();
    return null;
  }
}

async function getCustomSoundUrl(updatedAt: number | null) {
  const data = await getCustomReminderSound();
  if (!data) {
    return null;
  }

  const key = `${data.file_name}:${data.bytes.length}:${updatedAt ?? 'legacy'}`;
  if (customSoundCache?.key === key) {
    return customSoundCache.url;
  }

  if (customSoundCache) {
    URL.revokeObjectURL(customSoundCache.url);
    customSoundCache = null;
  }

  const bytes = new Uint8Array(data.bytes);
  const url = URL.createObjectURL(new Blob([bytes], { type: data.mime_type || 'audio/mpeg' }));
  customSoundCache = { key, url };
  return url;
}

function stopPreviewAudio() {
  const notificationId = previewNotificationId;
  previewNotificationId = null;
  if (notificationId) {
    unregisterNotificationSound(notificationId);
  }
  previewSoundStop?.();
  previewSoundStop = null;
  if (previewObjectUrl) {
    URL.revokeObjectURL(previewObjectUrl);
    previewObjectUrl = null;
  }
}

async function showDesktopNotification(payload: ReminderPayload, settings: ReminderSoundSettings, notificationId: string) {
  try {
    await showStudyReminder(payload.title, payload.body, toastSoundId(settings), notificationId, payload.wakeWindow ?? false);
    return;
  } catch {
    // Continue to plugin/browser notification fallback.
  }

  try {
    const notification = await import('@tauri-apps/plugin-notification');
    let permitted = await notification.isPermissionGranted();

    if (!permitted) {
      const permission = await notification.requestPermission();
      permitted = permission === 'granted';
    }

    if (permitted) {
      const visibleNotification = new Notification(payload.title, notificationOptions(payload.body));
      bindBrowserNotificationStop(visibleNotification, notificationId);
      return;
    }
  } catch {
    // Fall through to browser notification.
  }

  if ('Notification' in window) {
    if (Notification.permission === 'default') {
      await Notification.requestPermission();
    }

    if (Notification.permission === 'granted') {
      const visibleNotification = new Notification(payload.title, { body: payload.body });
      bindBrowserNotificationStop(visibleNotification, notificationId);
    }
  }
}

function createNotificationId(prefix: string) {
  notificationSequence += 1;
  return `${prefix}:${Date.now()}:${notificationSequence}`;
}

function registerNotificationSound(notificationId: string, stop: SoundStop, timeoutMs: number | null) {
  stopNotificationSound(notificationId);
  ensureNotificationClosedListener();
  const timeoutId = timeoutMs === null
    ? null
    : window.setTimeout(() => stopNotificationSound(notificationId), timeoutMs);
  activeNotificationSounds.set(notificationId, { stop, timeoutId });
}

function stopNotificationSound(notificationId: string) {
  const activeSound = activeNotificationSounds.get(notificationId);
  if (!activeSound) {
    return;
  }
  unregisterNotificationSound(notificationId);
  activeSound.stop();
}

function unregisterNotificationSound(notificationId: string) {
  const activeSound = activeNotificationSounds.get(notificationId);
  if (!activeSound) {
    return;
  }
  if (activeSound.timeoutId !== null) {
    window.clearTimeout(activeSound.timeoutId);
  }
  activeNotificationSounds.delete(notificationId);
}

function stopPersistentAlarmAudio(key: string) {
  persistentAlarmSounds.get(key)?.();
  persistentAlarmSounds.delete(key);
}

function ensureNotificationClosedListener() {
  if (notificationClosedListenerStarted) {
    return;
  }
  notificationClosedListenerStarted = true;
  void import('@tauri-apps/api/event')
    .then(({ listen }) => listen<{ notification_id: string }>(REMINDER_NOTIFICATION_CLOSED_EVENT, (event) => {
      const notificationId = event.payload?.notification_id;
      if (notificationId) {
        stopNotificationSound(notificationId);
      }
    }))
    .catch(() => {
      notificationClosedListenerStarted = false;
    });
}

function bindBrowserNotificationStop(notification: Notification, notificationId: string) {
  notification.onclose = () => stopNotificationSound(notificationId);
  notification.onclick = () => stopNotificationSound(notificationId);
  notification.onerror = () => stopNotificationSound(notificationId);
}

function notificationOptions(body: string, sound?: string) {
  return sound ? ({ body, sound } as NotificationOptions & { sound: string }) : { body };
}

async function loadReminderSoundSettings() {
  try {
    return normalizeReminderSoundSettings(await getAppSettings());
  } catch {
    return defaultReminderSoundSettings;
  }
}

function normalizeReminderSoundSettings(settings?: Partial<ReminderSoundSettings>): ReminderSoundSettings {
  return {
    reminder_sound_source: normalizeReminderSoundSource(settings?.reminder_sound_source),
    reminder_sound_id: normalizeReminderSoundId(settings?.reminder_sound_id),
    reminder_sound_file_name: settings?.reminder_sound_file_name ?? null,
    reminder_sound_updated_at: settings?.reminder_sound_updated_at ?? null,
    reminder_sound_volume: normalizeReminderSoundVolume(settings?.reminder_sound_volume),
    reminder_sound_duration_seconds: normalizeReminderSoundDuration(settings?.reminder_sound_duration_seconds),
    reminder_quiet_hours_enabled: settings?.reminder_quiet_hours_enabled ?? false,
    reminder_quiet_hours_start: normalizeTimeOfDay(settings?.reminder_quiet_hours_start, '22:30'),
    reminder_quiet_hours_end: normalizeTimeOfDay(settings?.reminder_quiet_hours_end, '07:00'),
  };
}

function normalizeReminderSoundSource(value?: ReminderSoundSource) {
  return value === 'custom' ? 'custom' : 'builtin';
}

function normalizeReminderSoundId(value?: string): ReminderSoundId {
  return value === 'bright' || value === 'soft' || value === 'urgent' || value === 'short' ? value : 'classic';
}

function normalizeReminderSoundVolume(value?: number) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return 100;
  }
  return Math.min(100, Math.max(0, Math.round(value)));
}

function normalizeReminderSoundDuration(value?: number) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return 30;
  }
  return Math.min(300, Math.max(5, Math.round(value)));
}

function volumeToRatio(volume: number) {
  return normalizeReminderSoundVolume(volume) / 100;
}

function isQuietHoursActive(settings: ReminderSoundSettings, now = new Date()) {
  if (!settings.reminder_quiet_hours_enabled) {
    return false;
  }

  const start = timeOfDayToMinutes(settings.reminder_quiet_hours_start);
  const end = timeOfDayToMinutes(settings.reminder_quiet_hours_end);
  const current = now.getHours() * 60 + now.getMinutes();
  if (start === end) {
    return false;
  }
  return start < end
    ? current >= start && current < end
    : current >= start || current < end;
}

function normalizeTimeOfDay(value: string | undefined, fallback: string) {
  if (!value || !/^\d{2}:\d{2}$/.test(value)) {
    return fallback;
  }
  const [hour, minute] = value.split(':').map(Number);
  if (hour > 23 || minute > 59) {
    return fallback;
  }
  return value;
}

function timeOfDayToMinutes(value: string) {
  const [hour, minute] = value.split(':').map(Number);
  return hour * 60 + minute;
}

function toastSoundId(settings: ReminderSoundSettings) {
  if (isQuietHoursActive(settings) || settings.reminder_sound_volume <= 0) {
    return 'silent';
  }
  return settings.reminder_sound_source === 'builtin'
    ? normalizeReminderSoundId(settings.reminder_sound_id)
    : 'classic';
}

function silentNotificationSettings(settings: ReminderSoundSettings): ReminderSoundSettings {
  return {
    ...settings,
    reminder_sound_source: 'custom',
  };
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}

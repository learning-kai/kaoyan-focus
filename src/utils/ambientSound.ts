/**
 * 白噪音（环境音）混音的音源清单与状态规则。
 *
 * 这里全部是纯函数：不依赖 React、不依赖 Audio / Tauri 运行时，
 * 播放器服务、面板 UI 和 Node 测试脚本共用同一套规则。
 *
 * 音源文件放在 public/sounds/ambient/，随前端构建一起打进安装包，
 * 署名与协议同时记录在 NOTICE.md，并在面板「音源署名」里展示。
 */

export type AmbientSoundGroup = 'nature' | 'place' | 'noise';

export type AmbientSoundLicense = 'CC BY 4.0' | 'CC BY 3.0' | 'CC0' | 'Public Domain';

export type AmbientSoundDefinition = {
  id: string;
  /** 面板里显示的中文名。 */
  label: string;
  /** 原始音源标题，用于署名。 */
  sourceTitle: string;
  group: AmbientSoundGroup;
  /** public/sounds/ambient/ 下的文件名。 */
  file: string;
  author: string;
  sourceUrl: string;
  license: AmbientSoundLicense;
  licenseUrl: string;
};

export const AMBIENT_SOUND_BASE_PATH = '/sounds/ambient/';

/** 处理版音频（剪辑为可循环片段）的来源，署名时一并注明。 */
export const AMBIENT_SOUND_PROCESSED_BY = {
  name: 'Blankie',
  url: 'https://blankie.rest/credits',
} as const;

const CC_BY_4 = 'https://creativecommons.org/licenses/by/4.0/';
const CC_BY_3 = 'https://creativecommons.org/licenses/by/3.0/';
const CC0 = 'https://creativecommons.org/publicdomain/zero/1.0/';

export const AMBIENT_SOUNDS: readonly AmbientSoundDefinition[] = [
  { id: 'rain', label: '雨声', sourceTitle: 'Rain', group: 'nature', file: 'rain.m4a', author: 'alex36917', sourceUrl: 'https://freesound.org/s/524605/', license: 'CC BY 4.0', licenseUrl: CC_BY_4 },
  { id: 'storm', label: '雷雨', sourceTitle: 'Storm', group: 'nature', file: 'storm.m4a', author: 'Digifish music', sourceUrl: 'https://freesound.org/s/41739', license: 'CC BY 4.0', licenseUrl: CC_BY_4 },
  { id: 'waves', label: '海浪', sourceTitle: 'Waves', group: 'nature', file: 'waves.m4a', author: 'Luftrum', sourceUrl: 'https://freesound.org/s/48412/', license: 'CC BY 4.0', licenseUrl: CC_BY_4 },
  { id: 'wind', label: '风声', sourceTitle: 'Wind', group: 'nature', file: 'wind.m4a', author: 'felix.blume', sourceUrl: 'https://freesound.org/s/217506/', license: 'CC0', licenseUrl: CC0 },
  { id: 'stream', label: '溪流', sourceTitle: 'Stream', group: 'nature', file: 'stream.m4a', author: 'gluckose', sourceUrl: 'https://freesound.org/s/333987/', license: 'CC0', licenseUrl: CC0 },
  { id: 'birds', label: '鸟鸣', sourceTitle: 'Birds', group: 'nature', file: 'birds.m4a', author: 'kvgarlic', sourceUrl: 'https://freesound.org/s/156826/', license: 'CC0', licenseUrl: CC0 },
  { id: 'summer-night', label: '夏夜虫鸣', sourceTitle: 'Summer Night', group: 'nature', file: 'summer-night.m4a', author: 'Lisa Redfern', sourceUrl: 'https://soundbible.com/2083-Crickets-Chirping-At-Night.html', license: 'Public Domain', licenseUrl: 'https://soundbible.com/2083-Crickets-Chirping-At-Night.html' },
  { id: 'train', label: '火车', sourceTitle: 'Train', group: 'place', file: 'train.m4a', author: 'SDLx', sourceUrl: 'https://freesound.org/s/259988/', license: 'CC BY 3.0', licenseUrl: CC_BY_3 },
  { id: 'city', label: '城市', sourceTitle: 'City', group: 'place', file: 'city.m4a', author: 'gezortenplotz', sourceUrl: 'https://freesound.org/s/44796/', license: 'CC BY 3.0', licenseUrl: CC_BY_3 },
  { id: 'boat', label: '小船', sourceTitle: 'Boat', group: 'place', file: 'boat.m4a', author: 'Falcet', sourceUrl: 'https://freesound.org/s/439365/', license: 'CC0', licenseUrl: CC0 },
  { id: 'coffee-shop', label: '咖啡馆', sourceTitle: 'Coffee Shop', group: 'place', file: 'coffee-shop.m4a', author: 'stephan', sourceUrl: 'https://soundbible.com/1664-Restaurant-Ambiance.html', license: 'Public Domain', licenseUrl: 'https://soundbible.com/1664-Restaurant-Ambiance.html' },
  { id: 'fireplace', label: '壁炉', sourceTitle: 'Fireplace', group: 'place', file: 'fireplace.m4a', author: 'ezwa', sourceUrl: 'https://soundbible.com/1543-Fireplace.html', license: 'Public Domain', licenseUrl: 'https://soundbible.com/1543-Fireplace.html' },
  { id: 'pink-noise', label: '粉噪音', sourceTitle: 'Pink Noise', group: 'noise', file: 'pink-noise.m4a', author: 'Blankie', sourceUrl: 'https://blankie.rest/credits', license: 'CC0', licenseUrl: CC0 },
  { id: 'white-noise', label: '白噪音', sourceTitle: 'White Noise', group: 'noise', file: 'white-noise.m4a', author: 'Blankie', sourceUrl: 'https://blankie.rest/credits', license: 'CC0', licenseUrl: CC0 },
];

export const AMBIENT_SOUND_GROUP_LABELS: Record<AmbientSoundGroup, string> = {
  nature: '自然',
  place: '场景',
  noise: '噪音',
};

export const AMBIENT_SOUND_GROUP_ORDER: readonly AmbientSoundGroup[] = ['nature', 'place', 'noise'];

/** 单个音源刚被打开时的默认音量（0-100）。 */
export const AMBIENT_DEFAULT_SOUND_VOLUME = 60;

/** 总音量默认值（0-100）。 */
export const AMBIENT_DEFAULT_MASTER_VOLUME = 70;

export type AmbientSoundChannel = {
  active: boolean;
  volume: number;
};

export type AmbientMixState = {
  /** 总开关。关掉时保留每个音源的选择和音量，只是全部静音。 */
  enabled: boolean;
  masterVolume: number;
  sounds: Record<string, AmbientSoundChannel>;
};

export type AmbientPlaybackItem = {
  id: string;
  file: string;
  /** 实际写给 HTMLAudioElement.volume 的 0-1 比例。 */
  gain: number;
};

const SOUND_IDS = new Set(AMBIENT_SOUNDS.map((sound) => sound.id));

export function isAmbientSoundId(id: string): boolean {
  return SOUND_IDS.has(id);
}

export function findAmbientSound(id: string): AmbientSoundDefinition | undefined {
  return AMBIENT_SOUNDS.find((sound) => sound.id === id);
}

export function ambientSoundUrl(file: string): string {
  return AMBIENT_SOUND_BASE_PATH + encodeURIComponent(file);
}

/** 把任意输入收敛到 0-100 的整数音量，永不返回 NaN。 */
export function clampAmbientVolume(value: unknown, fallback = AMBIENT_DEFAULT_SOUND_VOLUME): number {
  const numeric = typeof value === 'number' ? value : typeof value === 'string' && value.trim() !== '' ? Number(value) : Number.NaN;
  if (!Number.isFinite(numeric)) return fallback;
  return Math.min(100, Math.max(0, Math.round(numeric)));
}

export function createDefaultAmbientMix(): AmbientMixState {
  const sounds: Record<string, AmbientSoundChannel> = {};
  for (const sound of AMBIENT_SOUNDS) {
    sounds[sound.id] = { active: false, volume: AMBIENT_DEFAULT_SOUND_VOLUME };
  }
  return { enabled: false, masterVolume: AMBIENT_DEFAULT_MASTER_VOLUME, sounds };
}

/**
 * 从本地存储恢复混音。
 *
 * 恢复选中的音源和音量，但总开关一律回到关闭：
 * 应用可能随 Windows 开机自启，启动时突然出声不合适，需要用户手动再打开。
 */
export function parseStoredAmbientMix(raw: string | null | undefined): AmbientMixState {
  const mix = createDefaultAmbientMix();
  if (!raw) return mix;

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return mix;
  }
  if (!parsed || typeof parsed !== 'object') return mix;

  const record = parsed as { masterVolume?: unknown; sounds?: unknown };
  mix.masterVolume = clampAmbientVolume(record.masterVolume, AMBIENT_DEFAULT_MASTER_VOLUME);

  if (record.sounds && typeof record.sounds === 'object') {
    for (const [id, value] of Object.entries(record.sounds as Record<string, unknown>)) {
      if (!isAmbientSoundId(id) || !value || typeof value !== 'object') continue;
      const channel = value as { active?: unknown; volume?: unknown };
      mix.sounds[id] = {
        active: channel.active === true,
        volume: clampAmbientVolume(channel.volume),
      };
    }
  }

  return mix;
}

/** 序列化混音。总开关不落盘，见 parseStoredAmbientMix。 */
export function serializeAmbientMix(mix: AmbientMixState): string {
  return JSON.stringify({ masterVolume: mix.masterVolume, sounds: mix.sounds });
}

function withChannel(mix: AmbientMixState, id: string, patch: Partial<AmbientSoundChannel>): AmbientMixState {
  const current = mix.sounds[id] ?? { active: false, volume: AMBIENT_DEFAULT_SOUND_VOLUME };
  return { ...mix, sounds: { ...mix.sounds, [id]: { ...current, ...patch } } };
}

/**
 * 切换单个音源。
 *
 * 打开一个音源时顺带打开总开关：用户点了某个声音就是想听到它，
 * 不应该再让他去找总开关。关闭音源不影响总开关。
 */
export function toggleAmbientSound(mix: AmbientMixState, id: string): AmbientMixState {
  if (!isAmbientSoundId(id)) return mix;
  const nextActive = !mix.sounds[id]?.active;
  const next = withChannel(mix, id, { active: nextActive });
  return nextActive ? { ...next, enabled: true } : next;
}

export function setAmbientSoundVolume(mix: AmbientMixState, id: string, volume: number): AmbientMixState {
  if (!isAmbientSoundId(id)) return mix;
  return withChannel(mix, id, { volume: clampAmbientVolume(volume, mix.sounds[id]?.volume) });
}

export function setAmbientMasterVolume(mix: AmbientMixState, volume: number): AmbientMixState {
  return { ...mix, masterVolume: clampAmbientVolume(volume, mix.masterVolume) };
}

export function setAmbientEnabled(mix: AmbientMixState, enabled: boolean): AmbientMixState {
  return { ...mix, enabled };
}

/** 取消全部音源的选择（保留各自音量），并关闭总开关。 */
export function clearAmbientMix(mix: AmbientMixState): AmbientMixState {
  const sounds: Record<string, AmbientSoundChannel> = {};
  for (const [id, channel] of Object.entries(mix.sounds)) {
    sounds[id] = { ...channel, active: false };
  }
  return { ...mix, enabled: false, sounds };
}

export function countActiveAmbientSounds(mix: AmbientMixState): number {
  return AMBIENT_SOUNDS.reduce((count, sound) => count + (mix.sounds[sound.id]?.active ? 1 : 0), 0);
}

/** 总开关打开且至少有一个音源被选中时，才算真正在出声。 */
export function isAmbientMixAudible(mix: AmbientMixState): boolean {
  return buildAmbientPlaybackPlan(mix).length > 0;
}

/**
 * 计算当前应该播放的音源及其实际增益。
 *
 * 增益 = 音源音量 × 总音量，均按 0-1 线性相乘。音量为 0 的音源不进入播放计划，
 * 这样拖到 0 时会真正暂停解码，而不是空转。
 */
export function buildAmbientPlaybackPlan(mix: AmbientMixState): AmbientPlaybackItem[] {
  if (!mix.enabled) return [];
  const master = clampAmbientVolume(mix.masterVolume, AMBIENT_DEFAULT_MASTER_VOLUME) / 100;
  if (master <= 0) return [];

  const plan: AmbientPlaybackItem[] = [];
  for (const sound of AMBIENT_SOUNDS) {
    const channel = mix.sounds[sound.id];
    if (!channel?.active) continue;
    const gain = (clampAmbientVolume(channel.volume) / 100) * master;
    if (gain <= 0) continue;
    plan.push({ id: sound.id, file: sound.file, gain: Math.round(gain * 1000) / 1000 });
  }
  return plan;
}

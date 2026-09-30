/**
 * 白噪音混音：规则 + 资源 + 接线校验。
 *
 * 1. 用 esbuild 把 src/utils/ambientSound.ts 转成 ESM 后跑纯函数断言；
 * 2. 校验每个音源文件确实存在于 public/sounds/ambient/，且 NOTICE.md 记录了署名；
 * 3. 对 Layout / 播放器做源码级接线断言，防止改动时漏掉某一环。
 */
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { transform } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

async function loadModule() {
  const source = await readSource('src/utils/ambientSound.ts');
  const { code } = await transform(source, { format: 'esm', loader: 'ts', target: 'node20' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const {
  AMBIENT_DEFAULT_MASTER_VOLUME,
  AMBIENT_DEFAULT_SOUND_VOLUME,
  AMBIENT_SOUND_GROUP_ORDER,
  AMBIENT_SOUNDS,
  ambientSoundUrl,
  buildAmbientPlaybackPlan,
  clampAmbientVolume,
  clearAmbientMix,
  countActiveAmbientSounds,
  createDefaultAmbientMix,
  isAmbientMixAudible,
  parseStoredAmbientMix,
  serializeAmbientMix,
  setAmbientEnabled,
  setAmbientMasterVolume,
  setAmbientSoundVolume,
  toggleAmbientSound,
} = await loadModule();

// ---------- 音源清单 ----------
const expectedIds = [
  'rain', 'storm', 'waves', 'wind', 'stream', 'birds', 'summer-night',
  'train', 'city', 'boat', 'coffee-shop', 'fireplace', 'pink-noise', 'white-noise',
];
assert.deepEqual(AMBIENT_SOUNDS.map((sound) => sound.id).sort(), [...expectedIds].sort(), '音源清单应包含全部 14 种声音');
assert.equal(new Set(AMBIENT_SOUNDS.map((sound) => sound.id)).size, AMBIENT_SOUNDS.length, '音源 id 不能重复');
assert.equal(new Set(AMBIENT_SOUNDS.map((sound) => sound.file)).size, AMBIENT_SOUNDS.length, '音源文件不能重复');
for (const sound of AMBIENT_SOUNDS) {
  assert.ok(AMBIENT_SOUND_GROUP_ORDER.includes(sound.group), `${sound.id} 的分组必须在展示顺序里`);
  assert.ok(sound.label && sound.author && sound.license, `${sound.id} 必须有中文名、作者和协议`);
  assert.match(sound.sourceUrl, /^https:\/\//, `${sound.id} 的来源必须是 https 链接`);
  assert.match(sound.licenseUrl, /^https:\/\//, `${sound.id} 的协议必须是 https 链接`);
  if (sound.license.startsWith('CC BY')) {
    assert.match(sound.licenseUrl, /creativecommons\.org\/licenses\/by\//, `${sound.id} 为 CC BY，协议链接必须指向对应条款`);
  }
}
assert.equal(ambientSoundUrl('coffee-shop.m4a'), '/sounds/ambient/coffee-shop.m4a');

// ---------- 音频文件与署名 ----------
const notice = await readSource('NOTICE.md');
for (const sound of AMBIENT_SOUNDS) {
  const info = await stat(resolve(root, 'public/sounds/ambient', sound.file));
  assert.ok(info.size > 50_000, `${sound.file} 应是有效音频文件`);
  assert.ok(notice.includes(`public/sounds/ambient/${sound.file}`), `NOTICE.md 必须登记 ${sound.file}`);
  assert.ok(notice.includes(sound.sourceUrl), `NOTICE.md 必须记录 ${sound.id} 的来源`);
}

// ---------- clampAmbientVolume ----------
assert.equal(clampAmbientVolume(55), 55);
assert.equal(clampAmbientVolume(-3), 0);
assert.equal(clampAmbientVolume(140), 100);
assert.equal(clampAmbientVolume(33.6), 34, '四舍五入到整数');
assert.equal(clampAmbientVolume('80'), 80, '字符串数字可接受');
assert.equal(clampAmbientVolume(Number.NaN), AMBIENT_DEFAULT_SOUND_VOLUME, 'NaN 回落默认值');
assert.equal(clampAmbientVolume('', 12), 12, '空串回落到给定默认值');
assert.equal(clampAmbientVolume(null, 12), 12);

// ---------- 默认状态 ----------
const initial = createDefaultAmbientMix();
assert.equal(initial.enabled, false, '默认不出声');
assert.equal(initial.masterVolume, AMBIENT_DEFAULT_MASTER_VOLUME);
assert.equal(countActiveAmbientSounds(initial), 0);
assert.deepEqual(buildAmbientPlaybackPlan(initial), []);
assert.equal(isAmbientMixAudible(initial), false);

// ---------- 切换与混音 ----------
let mix = toggleAmbientSound(initial, 'rain');
assert.equal(mix.sounds.rain.active, true);
assert.equal(mix.enabled, true, '打开一个声音会顺带打开总开关');
assert.equal(initial.sounds.rain.active, false, '状态更新必须是不可变的');

mix = toggleAmbientSound(mix, 'fireplace');
mix = setAmbientSoundVolume(mix, 'fireplace', 50);
mix = setAmbientMasterVolume(mix, 80);
const plan = buildAmbientPlaybackPlan(mix);
assert.deepEqual(plan.map((item) => item.id), ['rain', 'fireplace'], '可以同时混合多种声音，按清单顺序输出');
assert.equal(plan.find((item) => item.id === 'rain').gain, 0.48, '增益 = 音源 60% × 总音量 80%');
assert.equal(plan.find((item) => item.id === 'fireplace').gain, 0.4, '增益 = 音源 50% × 总音量 80%');
assert.ok(plan.every((item) => item.gain > 0 && item.gain <= 1), '增益始终在 (0, 1]');
assert.equal(isAmbientMixAudible(mix), true);

// 总开关关掉：保留选择，但不出声
const paused = setAmbientEnabled(mix, false);
assert.equal(countActiveAmbientSounds(paused), 2, '关闭总开关不清空选择');
assert.deepEqual(buildAmbientPlaybackPlan(paused), []);
assert.equal(isAmbientMixAudible(paused), false);
assert.equal(buildAmbientPlaybackPlan(setAmbientEnabled(paused, true)).length, 2, '重新打开恢复原混音');

// 关闭单个声音不影响总开关
const oneOff = toggleAmbientSound(mix, 'rain');
assert.equal(oneOff.enabled, true);
assert.deepEqual(buildAmbientPlaybackPlan(oneOff).map((item) => item.id), ['fireplace']);

// 音量为 0 不进入播放计划
assert.deepEqual(buildAmbientPlaybackPlan(setAmbientSoundVolume(oneOff, 'fireplace', 0)), []);
assert.deepEqual(buildAmbientPlaybackPlan(setAmbientMasterVolume(mix, 0)), [], '总音量为 0 时整体静音');

// 未知 id 不改变状态
assert.equal(toggleAmbientSound(mix, 'unknown'), mix);
assert.equal(setAmbientSoundVolume(mix, 'unknown', 10), mix);

// 全部取消
const cleared = clearAmbientMix(mix);
assert.equal(countActiveAmbientSounds(cleared), 0);
assert.equal(cleared.enabled, false);
assert.equal(cleared.sounds.fireplace.volume, 50, '全部取消保留各自音量');

// ---------- 持久化 ----------
const restored = parseStoredAmbientMix(serializeAmbientMix(mix));
assert.equal(restored.enabled, false, '启动恢复时总开关一律关闭，避免开机自启突然出声');
assert.equal(restored.masterVolume, 80);
assert.equal(restored.sounds.rain.active, true, '恢复之前选中的声音');
assert.equal(restored.sounds.fireplace.volume, 50, '恢复各自音量');
assert.ok(!serializeAmbientMix(mix).includes('enabled'), '总开关不落盘');

assert.deepEqual(parseStoredAmbientMix(null), createDefaultAmbientMix());
assert.deepEqual(parseStoredAmbientMix('not json'), createDefaultAmbientMix(), '损坏数据回落默认');
assert.deepEqual(parseStoredAmbientMix('[1,2]').masterVolume, AMBIENT_DEFAULT_MASTER_VOLUME);
const dirty = parseStoredAmbientMix(
  JSON.stringify({ masterVolume: 999, sounds: { rain: { active: 'yes', volume: -5 }, ghost: { active: true, volume: 50 } } }),
);
assert.equal(dirty.masterVolume, 100, '越界总音量被钳制');
assert.equal(dirty.sounds.rain.active, false, '非布尔 active 视为关闭');
assert.equal(dirty.sounds.rain.volume, 0, '越界音量被钳制');
assert.equal(dirty.sounds.ghost, undefined, '未知音源被丢弃');
assert.equal(Object.keys(dirty.sounds).length, AMBIENT_SOUNDS.length, '恢复后每个音源都有通道');

// ---------- 接线 ----------
const layout = await readSource('src/components/Layout.tsx');
assert.ok(layout.includes("import AmbientSoundControl from './AmbientSoundControl'"), 'Layout 必须挂载白噪音入口');
assert.ok(layout.includes('<AmbientSoundControl />'), 'Layout 必须渲染白噪音入口');

const player = await readSource('src/services/ambientSoundPlayer.ts');
assert.ok(player.includes('audio.loop = true'), '音源必须循环播放');
assert.ok(player.includes('window.setInterval'), '淡入淡出必须用定时器，托盘隐藏时 rAF 会停');
assert.ok(!player.includes('requestAnimationFrame'), '淡入淡出不能依赖 rAF');

const widget = await readSource('src/pages/FocusWidgetPage.tsx');
assert.ok(!widget.includes('ambientSound'), '专注悬浮窗不应引入白噪音播放器，避免双窗口同时出声');

console.log('ambient sound tests passed');

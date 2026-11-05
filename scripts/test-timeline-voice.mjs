/**
 * 时间轴任务语音播报（Edge 在线音色 + 系统语音回落）的接线与纯函数校验。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { transform } from 'esbuild';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

async function loadModule(relativePath) {
  const source = await readSource(relativePath);
  const { code } = await transform(source, { format: 'esm', loader: 'ts', target: 'node20' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

const voice = await loadModule('src/utils/timelineVoice.ts');
const edge = await loadModule('src/utils/edgeTts.ts');

// ---------- 时间窗口与去重 ----------
assert.equal(voice.TIMELINE_VOICE_LEAD_MINUTES, 1, '固定为开始前 1 分钟');
const keyA = voice.buildTimelineVoiceKey(12, '2026-11-04', 510);
assert.equal(keyA, voice.buildTimelineVoiceKey(12, '2026-11-04', 510));
assert.match(keyA, /^2026-11-04:/);

// ---------- 播报文案 ----------
const spoken = voice.buildTimelineVoiceText({
  title: '套卷训练', subjectName: '数学', startMinute: 510, endMinute: 690,
});
assert.match(spoken, /一分钟后开始/);
assert.match(spoken, /数学/);
assert.match(spoken, /套卷训练/);
assert.match(spoken, /08:30/);
assert.match(spoken, /11:30/);
assert.match(spoken, /3 小时/);
assert.ok(!spoken.includes('undefined'));
assert.equal(voice.formatSpokenMinute(510), '上午8点30分');
assert.equal(voice.formatSpokenMinute(1200), '晚上8点整');
assert.equal(voice.categoryLabelForVoice('math'), '数学');

// ---------- 语速 ----------
assert.equal(voice.normalizeVoiceRate(100), 1);
assert.equal(voice.normalizeVoiceRate(50), 0.5);
assert.equal(voice.normalizeVoiceRate(200), 2);
assert.equal(voice.normalizeVoiceRate(undefined), 1);

// ---------- 引擎与音色规范化 ----------
assert.equal(voice.normalizeVoiceEngine('online'), 'online');
assert.equal(voice.normalizeVoiceEngine('system'), 'system');
assert.equal(voice.normalizeVoiceEngine(undefined), 'online', '默认走在线语音');
assert.equal(voice.normalizeVoiceEngine('bogus'), 'online');
assert.equal(voice.DEFAULT_ONLINE_VOICE, 'zh-CN-XiaoxiaoNeural', '默认音色为晓晓');
assert.equal(voice.normalizeOnlineVoice(''), 'zh-CN-XiaoxiaoNeural');
assert.equal(voice.normalizeOnlineVoice('zh-CN-YunxiNeural'), 'zh-CN-YunxiNeural');
assert.equal(voice.normalizeOnlineVoice('  zh-CN-YunxiNeural  '), 'zh-CN-YunxiNeural');
assert.equal(voice.normalizeOnlineVoice('<script>'), 'zh-CN-XiaoxiaoNeural', '非法字符应回落，防止 SSML 注入');
assert.equal(voice.normalizeOnlineVoice(null), 'zh-CN-XiaoxiaoNeural');

// ---------- 在线音色列表 ----------
const rawVoices = [
  { ShortName: 'en-US-EmmaMultilingualNeural', Name: 'Microsoft Emma', Locale: 'en-US', Gender: 'Female' },
  { ShortName: 'zh-CN-XiaoxiaoNeural', Name: 'Microsoft Xiaoxiao Online (Natural) - Chinese (Mainland)', Locale: 'zh-CN', Gender: 'Female' },
  { ShortName: 'zh-CN-YunxiNeural', Name: 'Microsoft Yunxi Online (Natural) - Chinese (Mainland)', Locale: 'zh-CN', Gender: 'Male' },
  { ShortName: 'zh-CN-liaoning-XiaobeiNeural', Name: 'Microsoft Xiaobei', Locale: 'zh-CN', Gender: 'Female' },
  { ShortName: 'zh-TW-HsiaoChenNeural', Name: 'Microsoft HsiaoChen', Locale: 'zh-TW', Gender: 'Female' },
  { ShortName: '', Name: 'empty', Locale: 'zh-CN', Gender: 'Female' },
];
const options = edge.buildOnlineVoiceOptions(rawVoices);
assert.equal(options.length, 4, '只保留中文语音，且剔除无名项');
assert.ok(!options.some((v) => v.shortName === 'en-US-EmmaMultilingualNeural'), '英文语音不进中文列表');
assert.equal(options[0].label, '晓晓 · 女声', '应有中文友好名');
assert.equal(options.find((v) => v.shortName === 'zh-CN-liaoning-XiaobeiNeural').label, '小北（东北话） · 女声');
assert.ok(options.findIndex((v) => v.locale === 'zh-CN') < options.findIndex((v) => v.locale === 'zh-TW'), '大陆普通话排前面');
assert.equal(edge.buildOnlineVoiceOptions('not an array').length, 0);

// ---------- SSML ----------
assert.equal(edge.escapeSsml('A & B <tag> "x"'), 'A &amp; B &lt;tag&gt; &quot;x&quot;');
assert.equal(edge.ssmlRateOffset(100), '+0%');
assert.equal(edge.ssmlRateOffset(120), '+20%');
assert.equal(edge.ssmlRateOffset(80), '-20%');
assert.equal(edge.ssmlRateOffset(10), '-50%', '越界应钳制');
const ssml = edge.buildSsml({ text: '数学 & 英语', voice: 'zh-CN-YunxiNeural', ratePercent: 120 });
assert.match(ssml, /<voice name="zh-CN-YunxiNeural">/);
assert.match(ssml, /rate="\+20%"/);
assert.match(ssml, /数学 &amp; 英语/, '特殊字符必须转义');

// ---------- Sec-MS-GEC ----------
const gec = await edge.computeSecMsGec(Date.UTC(2026, 10, 5, 0, 0, 0));
assert.match(gec, /^[0-9A-F]{64}$/, '应是 64 位大写 SHA-256');
const sameWindowA = await edge.computeSecMsGec(Date.UTC(2026, 10, 5, 0, 1, 0));
assert.equal(gec, sameWindowA, '同一 5 分钟窗口签名应一致');

// ---------- 后端设置 ----------
const settingsRs = await readSource('src-tauri/src/commands/settings.rs');
assert.match(settingsRs, /TIMELINE_ONLINE_VOICE_KEY/, '应有在线音色设置键');
assert.match(settingsRs, /TIMELINE_VOICE_ENGINE_KEY/, '应有引擎设置键');
assert.match(settingsRs, /pub timeline_online_voice: String,/, '结构体应包含在线音色');
assert.match(settingsRs, /pub timeline_voice_engine: String,/, '结构体应包含引擎');
assert.match(settingsRs, /timeline_voice_engine: "online"\.to_string\(\)/, '默认在线语音');
assert.match(settingsRs, /"system" => "system"\.to_string\(\)/, '引擎值应被收敛');
assert.match(settingsRs, /TIMELINE_ONLINE_VOICE_KEY,\s*&normalized\.timeline_online_voice/, '音色应落库');
assert.match(settingsRs, /TIMELINE_VOICE_ENGINE_KEY,\s*&normalized\.timeline_voice_engine/, '引擎应落库');
assert.ok(!/timeline_voice_name/.test(settingsRs), '旧的系统语音名设置应已移除');

// ---------- CSP ----------
const conf = await readSource('src-tauri/tauri.conf.json');
assert.match(conf, /connect-src[^;]*\bwss:/, 'CSP 应放行 wss，否则在线语音连不上');

// ---------- 前端类型与默认值 ----------
const settingsTs = await readSource('src/types/settings.ts');
assert.match(settingsTs, /timeline_online_voice: string;/);
assert.match(settingsTs, /timeline_voice_engine: 'online' \| 'system';/);
const settingsPage = await readSource('src/pages/SettingsPage.tsx');
assert.match(settingsPage, /timeline_online_voice: '',/);
assert.match(settingsPage, /timeline_voice_engine: 'online',/);

// ---------- hook 接线 ----------
const hook = await readSource('src/hooks/useAppBackgroundTasks.ts');
assert.match(hook, /speakTimelineTask\(/);
assert.match(hook, /TIMELINE_VOICE_LEAD_MINUTES/);
assert.match(hook, /buildTimelineVoiceKey/);
assert.match(hook, /spokenKeys/, '语音去重独立于铃声');
assert.match(hook, /engine: settings\.timeline_voice_engine/, '应传引擎');
assert.match(hook, /onlineVoice: settings\.timeline_online_voice/, '应传在线音色');
assert.match(hook, /ratePercent: settings\.timeline_voice_rate/, '应传语速百分比');

// ---------- 面板接线 ----------
const panel = await readSource('src/pages/settings/BasicSettingsPanel.tsx');
assert.match(panel, /fetchOnlineVoices/, '应拉取在线音色');
assert.match(panel, /语音来源/, '应有引擎切换');
assert.match(panel, /在线音色/, '应有音色下拉');
assert.match(panel, /timeline_voice_engine: 'online'/, '应能切到在线');
assert.match(panel, /timeline_voice_engine: 'system'/, '应能切到系统语音');
assert.match(panel, /timeline_online_voice: event\.target\.value/, '选择应写回设置');
assert.match(panel, /刷新/, '应有刷新按钮');
assert.match(panel, /void speakTimelineTask\(/, '试听应使用同一个播报函数');
assert.match(panel, /onlineVoice: settings\.timeline_online_voice/, '试听应带当前音色');

// ---------- 回落不变式 ----------
const timelineVoiceSrc = await readSource('src/utils/timelineVoice.ts');
assert.match(timelineVoiceSrc, /synthesizeOnlineSpeech/, '主通道是在线合成');
assert.match(timelineVoiceSrc, /return speakWithSystemVoice\(options\)/, '在线失败应回落系统语音');
assert.match(timelineVoiceSrc, /speechSynthesis/, '系统语音兜底仍保留');
assert.ok(
  timelineVoiceSrc.indexOf('synthesizeOnlineSpeech') < timelineVoiceSrc.indexOf('return speakWithSystemVoice'),
  '在线通道必须先于回落被尝试',
);

console.log('Timeline online voice assertions passed');

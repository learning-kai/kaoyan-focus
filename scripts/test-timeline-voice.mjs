/**
 * 时间轴任务语音播报（Edge 在线音色 + 系统语音回落）的接线与纯函数校验。
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readSource = (relativePath) => readFile(resolve(root, relativePath), 'utf8');

/**
 * 把 TS 模块转成可 import 的 ESM。
 *
 * 用 bundle 模式并把 `tauriInvoke` 打桩：纯函数测试不需要真的连 Tauri，
 * 但 `timelineVoice.ts` 现在会调用后端 SAPI 命令，必须能解析到这个依赖。
 */
async function loadModule(relativePath) {
  const stubPlugin = {
    name: 'stub-tauri-invoke',
    setup(build) {
      build.onResolve({ filter: /tauriInvoke$/ }, (args) => ({ path: args.path, namespace: 'stub' }));
      build.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
        contents: [
          'export function invokeCommand() { return Promise.resolve(); }',
          'export function isTauriRuntime() { return true; }',
          'export const DESKTOP_RUNTIME_MESSAGE = "";',
        ].join(String.fromCharCode(10)),
        loader: 'js',
      }));
    },
  };
  const { outputFiles } = await build({
    entryPoints: [resolve(root, relativePath)],
    bundle: true,
    write: false,
    format: 'esm',
    platform: 'neutral',
    target: 'node20',
    plugins: [stubPlugin],
  });
  // 写临时文件再 import：base64 data URL 在含中文时容易被 Node 的解码路径弄坏。
  const code = outputFiles[0].text;
  const tmpDir = join(tmpdir(), 'kaoyan-timeline-voice-test');
  await mkdir(tmpDir, { recursive: true });
  const tmpFile = join(tmpDir, `${relativePath.replace(/[\/.]/g, '_')}-${Date.now()}.mjs`);
  await writeFile(tmpFile, code, 'utf8');
  return import(pathToFileURL(tmpFile).href);
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
assert.equal(voice.normalizeVoiceEngine(undefined), 'system', '默认走系统语音：在线依赖 WSS，代理下会哑');
assert.equal(voice.normalizeVoiceEngine('bogus'), 'system');
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
assert.match(settingsRs, /timeline_voice_engine: "system"\.to_string\(\)/, '默认系统语音');
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
assert.match(settingsPage, /timeline_voice_engine: 'system',/, '前端默认系统语音');

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
assert.match(panel, /handlePreviewSpeech/, '试听应走统一的播报处理函数');
assert.match(panel, /onlineVoice: settings\.timeline_online_voice/, '试听应带当前音色');

// ---------- 回落不变式 ----------
const timelineVoiceSrc = await readSource('src/utils/timelineVoice.ts');
assert.match(timelineVoiceSrc, /synthesizeOnlineSpeech/, '主通道是在线合成');
assert.match(timelineVoiceSrc, /await speakWithSystemVoice\(options\)/, '在线失败应回落系统语音');
assert.match(timelineVoiceSrc, /synthesize_timeline_speech/, '应调用后端合成命令');
// 关键不变式：后端只合成、播放必须在 WebView 里，否则声音到不了音箱。
assert.match(timelineVoiceSrc, /new Audio\(url\)/, '必须用 Audio 播放合成的 WAV');
assert.match(timelineVoiceSrc, /URL\.createObjectURL/, '应把字节转成 Blob URL 播放');
assert.ok(
  !/speak_timeline_task/.test(timelineVoiceSrc),
  '不应再让 Rust 直接朗读：应用进程音频到不了音箱',
);
// 只检查代码本体：注释里说明「为何不用」是合理的，不算违规。
const timelineVoiceCode = timelineVoiceSrc
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/^\s*\/\/.*$/gm, '');
assert.ok(
  !/speechSynthesis/.test(timelineVoiceCode),
  '代码里不应再用浏览器 speechSynthesis：WebView2 里 getVoices() 返回空数组且静默触发 end',
);
// 控制流顺序：在线分支（try）必须在前，系统语音调用在其之后，作为失败兜底。
const onlineTryAt = timelineVoiceCode.indexOf('synthesizeOnlineSpeech');
const systemCallAt = timelineVoiceCode.indexOf('await speakWithSystemVoice(options)');
assert.ok(onlineTryAt > 0 && systemCallAt > onlineTryAt, '在线通道必须先于系统语音回落被尝试');
assert.ok(
  /catch \{[\s\S]{0,200}?\}/.test(timelineVoiceCode.slice(onlineTryAt, systemCallAt)),
  '在线失败必须被 try/catch 接住后才回落',
);

console.log('Timeline online voice assertions passed');

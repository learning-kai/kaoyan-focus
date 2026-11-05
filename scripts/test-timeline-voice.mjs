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
// 关键：TTS 不能念「08:30」这种数字格式（会读成「零八冒号三十」）。
assert.ok(!/\d{2}:\d{2}/.test(spoken), `不应出现数字时间格式：${spoken}`);
assert.match(spoken, /上午八点半到十一点半/, `时间应口语化：${spoken}`);
assert.match(spoken, /三小时/, `时长应用汉字：${spoken}`);
assert.ok(!spoken.includes('undefined'));

// 口语化时间规则
assert.equal(voice.formatSpokenMinute(510), '上午八点半');
assert.equal(voice.formatSpokenMinute(480), '上午八点', '整点不带「分」');
assert.equal(voice.formatSpokenMinute(495), '上午八点十五分');
assert.equal(voice.formatSpokenMinute(720), '中午十二点', '12 点说「中午」');
assert.equal(voice.formatSpokenMinute(1290), '晚上九点半');
assert.equal(voice.formatSpokenMinute(0), '凌晨十二点', '0 点归凌晨');

// 同时段不重复前缀
const sameBlock = voice.buildTimelineVoiceText({
  title: '背单词', subjectName: '英语', startMinute: 540, endMinute: 615,
});
assert.match(sameBlock, /从上午九点到十点十五分/, `同时段应省略重复前缀：${sameBlock}`);
assert.ok(!/到上午/.test(sameBlock), '同一时段内不应重复「上午」');

// 跨时段仍要各自带前缀
const cross = voice.buildTimelineVoiceText({
  title: '午休', subjectName: null, startMinute: 720, endMinute: 780,
});
assert.match(cross, /从中午十二点到下午一点/, `跨时段应带各自前缀：${cross}`);
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

// ---------- 本地 Piper 引擎不变式 ----------
const voiceRs = await readSource('src-tauri/src/commands/timeline_voice.rs');
const NL = String.fromCharCode(10);
const voiceRsCode = voiceRs
  .split(NL)
  .filter((line) => !line.trimStart().startsWith('//'))
  .join(NL);
assert.match(voiceRs, /piper\.exe/, '应调用随包分发的 Piper 引擎');
assert.match(voiceRs, /zh_CN-huayan-medium\.onnx/, '应使用随包的中文语音模型');
assert.match(voiceRs, /ensure_ascii_runtime/, '必须镜像到 ASCII 路径');
assert.match(
  voiceRs,
  /Illegal byte sequence|非 ASCII/,
  '应说明为什么必须走 ASCII 路径（espeak-ng 读不了中文路径）',
);
assert.match(voiceRs, /CREATE_NO_WINDOW|0x08000000/, '不得弹出控制台窗口');
assert.ok(
  !/powershell/i.test(voiceRsCode),
  '不应再通过 PowerShell 调系统语音：那是拼接式机械音，且路径/转义问题多',
);
// 只检查代码本体：文件头注释记录「为什么放弃 ISpVoice」是必要的，不算违规。
assert.ok(
  !/speak_blocking|ISpVoice/.test(voiceRsCode),
  '不应在 Rust 进程内直接朗读（音频到不了音箱）',
);

// 资源必须随包分发，否则用户装完没有引擎。
const tauriConf = await readSource('src-tauri/tauri.conf.json');
assert.match(tauriConf, /resources\/piper/, 'Piper 运行时应打进安装包');
// 必须整目录映射：`resources/piper/espeak-ng-data/*` 这种 glob 不递归子目录，
// 会导致 voices/lang 漏打包，piper 静默产出 0 字节。
assert.match(
  tauriConf,
  /"resources"\s*:\s*\{\s*"resources\/piper"\s*:\s*"resources\/piper"/,
  '应以整目录映射打包 Piper，避免 espeak 子目录漏打',
);

console.log('Timeline online voice assertions passed');

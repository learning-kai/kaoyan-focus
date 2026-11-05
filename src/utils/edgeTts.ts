/**
 * 微软 Edge 在线语音（Edge TTS）。
 *
 * 无需 API Key：走 Edge 浏览器的「大声朗读」公开 WebSocket 端点，
 * 直接拿到 mp3 音频流。音色是网上那批 Natural 神经语音（晓晓、云希、晓伊、
 * 辽宁小北、陕西小妮等），比 Windows 自带的系统语音丰富得多。
 *
 * 协议要点（与 edge-tts 生态一致）：
 * - 语音列表：GET /voices/list，返回全部可选用音色；
 * - 合成：WSS /edge/v1，先发 speech.config，再发 SSML，服务端回二进制音频分片；
 * - 鉴权：TrustedClientToken + Sec-MS-GEC（基于时间取整的 SHA-256）+ 版本号。
 *
 * 这个模块只依赖浏览器原生能力（fetch / WebSocket / Web Crypto / Audio），不引第三方包。
 * 所有网络失败都往上抛，由调用方决定是否回落到系统语音。
 */

const TRUSTED_CLIENT_TOKEN = '6A5AA1D4EAFF4E9FB37E23D68491D6F4';
const BASE_URL = 'speech.platform.bing.com/consumer/speech/synthesize/readaloud';
const VOICE_LIST_URL = `https://${BASE_URL}/voices/list?trustedclienttoken=${TRUSTED_CLIENT_TOKEN}`;
/** 与服务端 Chromium 版本对齐，用于 Sec-MS-GEC-Version。 */
const CHROMIUM_FULL_VERSION = '143.0.3650.75';
const WINDOWS_EPOCH_SECONDS = 11644473600;
const S_TO_NS = 10000000;
/** GEC 时间取整粒度：5 分钟（300 秒 = 3e9 个 100ns 单位）。 */
const GEC_ROUND_NS = 3000000000;

const USER_AGENT =
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ` +
  `Chrome/${CHROMIUM_FULL_VERSION.split('.')[0]}.0.0.0 Safari/537.36 ` +
  `Edg/${CHROMIUM_FULL_VERSION.split('.')[0]}.0.0.0`;

export type OnlineVoice = {
  /** 传给 SSML 的 ShortName，例如 zh-CN-XiaoxiaoNeural。 */
  shortName: string;
  /** 面向用户的中文展示名。 */
  label: string;
  locale: string;
  gender: 'Female' | 'Male' | string;
  /** 是否中文语音。 */
  chinese: boolean;
};

/** 把 Edge 返回的 FriendlyName 收成短标签，避免下拉框里全是「Microsoft ... Online (Natural)」。 */
export function friendlyVoiceName(shortName: string, rawName: string, gender: string): string {
  const genderLabel = gender === 'Female' ? '女声' : gender === 'Male' ? '男声' : '';
  // 常见音色的中文名，便于一眼认出；没匹配上就退回 ShortName 里的标识。
  const known: Record<string, string> = {
    'zh-CN-XiaoxiaoNeural': '晓晓',
    'zh-CN-XiaoyiNeural': '晓伊',
    'zh-CN-YunjianNeural': '云健',
    'zh-CN-YunxiNeural': '云希',
    'zh-CN-YunxiaNeural': '云夏',
    'zh-CN-YunyangNeural': '云扬',
    'zh-CN-liaoning-XiaobeiNeural': '小北（东北话）',
    'zh-CN-shaanxi-XiaoniNeural': '小妮（陕西话）',
    'zh-HK-HiuGaaiNeural': '曉佳（粤语）',
    'zh-HK-HiuMaanNeural': '曉曼（粤语）',
    'zh-HK-WanLungNeural': '雲龍（粤语）',
    'zh-TW-HsiaoChenNeural': '曉臻（台湾）',
    'zh-TW-YunJheNeural': '雲哲（台湾）',
    'zh-TW-HsiaoYuNeural': '曉雨（台湾）',
  };
  const base = known[shortName] ?? rawName.replace(/^Microsoft\s+/i, '').replace(/\s+Online.*$/i, '').trim();
  return genderLabel ? `${base} · ${genderLabel}` : base;
}

/** 从 voices/list 的原始 JSON 构造可选列表；只保留中文语音并按展示名排序。 */
export function buildOnlineVoiceOptions(raw: unknown): OnlineVoice[] {
  if (!Array.isArray(raw)) return [];
  const options: OnlineVoice[] = [];
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue;
    const record = item as Record<string, unknown>;
    const shortName = typeof record.ShortName === 'string' ? record.ShortName : '';
    const locale = typeof record.Locale === 'string' ? record.Locale : '';
    if (!shortName || !/^zh-/i.test(locale)) continue;
    const rawName = typeof record.Name === 'string' ? record.Name : shortName;
    const gender = typeof record.Gender === 'string' ? record.Gender : '';
    options.push({
      shortName,
      label: friendlyVoiceName(shortName, rawName, gender),
      locale,
      gender,
      chinese: true,
    });
  }
  // 排序：常用标准普通话 → 其它普通话方言 → 粤语/台湾。
  // 方言音色的 Locale 同样是 zh-CN，只能靠 ShortName 里的地区段区分，
  // 且中文名的拼音顺序会把「小北」排到「晓晓」前面，所以用固定权重而不是按名字排。
  const preferredOrder = [
    'zh-CN-XiaoxiaoNeural',
    'zh-CN-YunxiNeural',
    'zh-CN-XiaoyiNeural',
    'zh-CN-YunjianNeural',
    'zh-CN-YunxiaNeural',
    'zh-CN-YunyangNeural',
  ];
  const rank = (shortName: string, locale: string) => {
    const preferred = preferredOrder.indexOf(shortName);
    if (preferred >= 0) return preferred;
    if (locale === 'zh-CN') return 100;
    if (locale.startsWith('zh-HK') || locale === 'zh-TW') return 200;
    return 300;
  };
  options.sort((a, b) => rank(a.shortName, a.locale) - rank(b.shortName, b.locale));
  return options;
}

/** 拉取在线语音列表。失败时抛错，由调用方回落。 */
export async function fetchOnlineVoices(): Promise<OnlineVoice[]> {
  const response = await fetch(VOICE_LIST_URL, {
    headers: {
      'User-Agent': USER_AGENT,
      Accept: '*/*',
    },
  });
  if (!response.ok) throw new Error(`语音列表请求失败（${response.status}）`);
  return buildOnlineVoiceOptions(await response.json());
}

/** 计算 Sec-MS-GEC：把当前时间转成 Windows FILETIME 并向下取整到 5 分钟，再拼 token 做 SHA-256。 */
export async function computeSecMsGec(nowMs = Date.now()): Promise<string> {
  const seconds = Math.floor(nowMs / 1000) + WINDOWS_EPOCH_SECONDS;
  const ticks = seconds * S_TO_NS;
  const rounded = ticks - (ticks % GEC_ROUND_NS);
  const input = `${rounded}${TRUSTED_CLIENT_TOKEN}`;
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
    .toUpperCase();
}

function randomHexId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes).map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** 时间戳格式：Edge 要求 RFC 1123 的 UTC 字符串，如 "Wed, 05 Nov 2026 00:00:00 GMT"。 */
function edgeTimestamp(): string {
  return new Date().toUTCString().replace('GMT', 'GMT');
}

function buildSpeechConfigMessage(): string {
  return (
    `X-Timestamp:${edgeTimestamp()}\r\n` +
    `Content-Type:application/json; charset=utf-8\r\n` +
    `Path:speech.config\r\n\r\n` +
    JSON.stringify({
      context: {
        synthesis: {
          audio: {
            metadataoptions: { sentenceBoundaryEnabled: false, wordBoundaryEnabled: false },
            outputFormat: 'audio-24khz-48kbitrate-mono-mp3',
          },
        },
      },
    }) +
    '\r\n'
  );
}

/** 转义 SSML 里的特殊字符，避免任务标题含 & < > 时把 XML 弄坏。 */
export function escapeSsml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

/** 把设置里的百分比整数换成 SSML 的 rate 偏移，例如 100 → "+0%"，120 → "+20%"。 */
export function ssmlRateOffset(percent: number): string {
  const clamped = Math.min(200, Math.max(50, Math.round(percent)));
  const offset = clamped - 100;
  return offset >= 0 ? `+${offset}%` : `${offset}%`;
}

export function buildSsml(args: { text: string; voice: string; ratePercent: number }): string {
  const rate = ssmlRateOffset(args.ratePercent);
  return (
    `<speak version="1.0" xmlns="http://www.w3.org/2001/10/Synthesis" xml:lang="zh-CN">` +
    `<voice name="${escapeSsml(args.voice)}">` +
    `<prosody pitch="+0Hz" rate="${rate}" volume="+0%">${escapeSsml(args.text)}</prosody>` +
    `</voice></speak>`
  );
}

function buildSsmlMessage(ssml: string): string {
  return (
    `X-RequestId:${randomHexId()}\r\n` +
    `Content-Type:application/ssml+xml\r\n` +
    `X-Timestamp:${edgeTimestamp()}Z\r\n` +
    `Path:ssml\r\n\r\n` +
    ssml
  );
}

/** 音频分片的固定头部长度：Edge 在每个二进制帧前加 2 字节长度前缀。 */
const AUDIO_HEADER_LENGTH = 2;

/**
 * 合成一段语音并返回可播放的 Blob。
 *
 * WSS 在部分环境（无网、企业代理、WebView 限制）会失败，此时抛错，
 * 由调用方 `speakTimelineTask` 回落到系统语音，保证提醒不哑。
 */
export async function synthesizeOnlineSpeech(args: {
  text: string;
  voice: string;
  ratePercent: number;
  timeoutMs?: number;
}): Promise<Blob> {
  const gec = await computeSecMsGec();
  const url =
    `wss://${BASE_URL}/edge/v1?TrustedClientToken=${TRUSTED_CLIENT_TOKEN}` +
    `&Sec-MS-GEC=${gec}&Sec-MS-GEC-Version=1-${CHROMIUM_FULL_VERSION}` +
    `&ConnectionId=${randomHexId()}`;

  return new Promise<Blob>((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.binaryType = 'arraybuffer';
    const audioChunks: Uint8Array[] = [];
    let settled = false;
    const timeoutMs = args.timeoutMs ?? 15000;
    const timer = window.setTimeout(() => finish(new Error('在线语音连接超时')), timeoutMs);

    function finish(error?: Error, blob?: Blob) {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      try { socket.close(); } catch { /* 已关闭 */ }
      if (error) reject(error);
      else if (blob && blob.size > 0) resolve(blob);
      else reject(new Error('在线语音未返回音频'));
    }

    socket.onopen = () => {
      socket.send(buildSpeechConfigMessage());
      // 稍微错开，避免服务端还没处理完 config 就收到 SSML。
      window.setTimeout(() => {
        if (!settled) socket.send(buildSsmlMessage(buildSsml({
          text: args.text,
          voice: args.voice,
          ratePercent: args.ratePercent,
        })));
      }, 200);
    };

    socket.onmessage = (event: MessageEvent) => {
      if (typeof event.data === 'string') {
        if (event.data.includes('Path:turn.end')) {
          finish(undefined, new Blob(audioChunks.map((chunk) => chunk.buffer as ArrayBuffer), { type: 'audio/mpeg' }));
        }
        return;
      }
      const buffer = event.data as ArrayBuffer;
      if (buffer.byteLength <= AUDIO_HEADER_LENGTH) return;
      audioChunks.push(new Uint8Array(buffer, AUDIO_HEADER_LENGTH));
    };

    socket.onerror = () => finish(new Error('在线语音连接失败'));
    socket.onclose = () => {
      if (!settled) {
        if (audioChunks.length > 0) {
          finish(undefined, new Blob(audioChunks.map((chunk) => chunk.buffer as ArrayBuffer), { type: 'audio/mpeg' }));
        } else {
          finish(new Error('在线语音连接被关闭'));
        }
      }
    };
  });
}

/** 播放一段 mp3 Blob；返回停止函数。 */
export function playAudioBlob(blob: Blob, volume = 1): () => void {
  const url = URL.createObjectURL(blob);
  const audio = new Audio(url);
  audio.volume = Math.min(1, Math.max(0, volume));
  audio.play().catch(() => undefined);
  return () => {
    audio.pause();
    URL.revokeObjectURL(url);
  };
}

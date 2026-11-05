/**
 * 时间轴任务语音播报。
 *
 * 主通道是微软 Edge 在线语音（见 edgeTts.ts）：晓晓、云希、晓伊、东北小北、
 * 陕西小妮等神经音色，无需 API Key，联网即用。
 * 在线通道失败（断网、代理、WebSocket 被拦）时自动回落到系统自带语音，保证提醒不哑。
 *
 * 纯函数（拼播报文案、去重键、参数规范化）单独导出，便于 Node 测试脚本直接断言。
 */

/** 提前多少分钟播报：需求固定为「开始前一分钟」。 */
import { invokeCommand } from '../services/tauriInvoke';

export const TIMELINE_VOICE_LEAD_MINUTES = 1;

/** 轮询间隔：30 秒一次，保证不会错过 T-1 分钟那个窗口。 */
export const TIMELINE_VOICE_CHECK_INTERVAL_MS = 30 * 1000;

/** 同一个任务在同一天只播一次。 */
export function buildTimelineVoiceKey(blockId: number, scheduleDate: string, startMinute: number) {
  return `${scheduleDate}:${blockId}:${startMinute}`;
}

/**
 * 把时间轴任务拼成一句自然的中文播报。
 *
 * 只包含「下一项是什么 + 几点开始」，不念备注（备注可能很长、也可能含不适合朗读的内容）。
 * 科目名由调用方传入，拼在标题前，便于听出「数学：套卷训练」这类结构。
 */
export function buildTimelineVoiceText(args: {
  title: string;
  subjectName?: string | null;
  startMinute: number;
  endMinute: number;
}): string {
  const title = args.title.trim();
  const subject = args.subjectName?.trim();
  const subjectPrefix = subject ? `${subject}，` : '';
  // 用口语化时间（「上午八点半」），不要用 08:30 这类数字格式 —— TTS 会念成「零八冒号三十」。
  const startHour = Math.floor(args.startMinute / 60);
  const endHour = Math.floor(args.endMinute / 60);
  const samePeriod = periodLabel(startHour) === periodLabel(endHour);
  // 同一时段内不重复前缀：说「从上午八点半到十一点半」，而不是「上午…到上午…」。
  const start = formatSpokenMinute(args.startMinute);
  const endText = samePeriod
    ? formatSpokenMinute(args.endMinute).slice(periodLabel(endHour).length)
    : formatSpokenMinute(args.endMinute);
  const duration = Math.max(0, args.endMinute - args.startMinute);
  const durationText = duration > 0 ? `，预计${formatDurationText(duration)}` : '';
  return `一分钟后开始：${subjectPrefix}${title}。时间从${start}到${endText}${durationText}，请准备。`;
}

/** 日历分类 key → 中文科目名，与 SchedulePage 的 categories 一致。 */
const CATEGORY_LABELS: Record<string, string> = {
  politics: '政治',
  english: '英语',
  math: '数学',
  major: '专业课',
  general: '通用',
};

export function categoryLabelForVoice(key: string): string {
  return CATEGORY_LABELS[key] ?? '通用';
}

/**
 * 把距 00:00 的分钟数转成口语说法，供 TTS 朗读。
 *
 * 直接念「08:30」这类数字格式，中文语音会读成「零八冒号三十」，非常别扭。
 * 这里按中文习惯产出：
 *   480  → 上午八点
 *   510  → 上午八点半
 *   495  → 上午八点十五分
 *   720  → 中午十二点
 *   1290 → 晚上九点半
 * 整点不带「分」，30 分说「半」，12 点单独说「中午」。
 */
export function formatSpokenMinute(minute: number): string {
  const safeMinute = Math.max(0, Math.min(1439, Math.floor(minute)));
  const hour = Math.floor(safeMinute / 60);
  const minutes = safeMinute % 60;

  const hour12 = hour % 12 === 0 ? 12 : hour % 12;

  if (minutes === 0) return `${periodLabel(hour)}${numberToChinese(hour12)}点`;
  if (minutes === 30) return `${periodLabel(hour)}${numberToChinese(hour12)}点半`;
  return `${periodLabel(hour)}${numberToChinese(hour12)}点${numberToChinese(minutes)}分`;
}

/** 时间段前缀。 */
function periodLabel(hour: number): string {
  if (hour < 6) return '凌晨';
  if (hour < 12) return '上午';
  if (hour === 12) return '中午';
  if (hour < 18) return '下午';
  return '晚上';
}

/** 1-59 的汉字写法，供 TTS 朗读（「8 点」不如「八点」稳）。 */
function numberToChinese(value: number): string {
  const digits = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
  const safe = Math.max(0, Math.min(99, Math.floor(value)));
  if (safe < 10) return digits[safe];
  if (safe < 20) return `十${safe % 10 === 0 ? '' : digits[safe % 10]}`;
  const tens = Math.floor(safe / 10);
  const ones = safe % 10;
  return `${digits[tens]}十${ones === 0 ? '' : digits[ones]}`;
}

function formatMinute(minute: number) {
  const safeMinute = Math.max(0, Math.min(1439, Math.floor(minute)));
  return `${String(Math.floor(safeMinute / 60)).padStart(2, '0')}:${String(safeMinute % 60).padStart(2, '0')}`;
}

function formatDurationText(minutes: number) {
  if (minutes < 60) return `${numberToChinese(minutes)}分钟`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0
    ? `${numberToChinese(hours)}小时`
    : `${numberToChinese(hours)}小时${numberToChinese(rest)}分钟`;
}

/** 「自动 / 在线默认音色」的取值：由后端推荐的晓晓。 */
export const DEFAULT_ONLINE_VOICE = 'zh-CN-XiaoxiaoNeural';

/** 播报引擎：在线（默认）或系统语音。 */
export type VoiceEngine = 'online' | 'system';

/**
 * 归一化播报引擎。
 *
 * 默认 `system` 现在指**本地 Piper 神经网络 TTS**（随安装包分发的
 * `zh_CN-huayan-medium` 中文音色）：完全离线、不依赖系统语音包、不联网，
 * 音质远好于 Windows SAPI 的机械音。
 *
 * `online` 仍保留为可选项（微软 Edge 的晓晓等），但它依赖 WebSocket，
 * 开启系统代理时容易连不上，因此不作为默认。
 */
export function normalizeVoiceEngine(value: unknown): VoiceEngine {
  return value === 'online' ? 'online' : 'system';
}

/** 规范化在线音色名；空串或非法值回落到默认音色。 */
export function normalizeOnlineVoice(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) return DEFAULT_ONLINE_VOICE;
  // Edge 的 ShortName 形如 zh-CN-XiaoxiaoNeural；不接受任意字符串，避免 SSML 注入。
  return /^[a-zA-Z0-9-]{1,64}$/.test(value.trim()) ? value.trim() : DEFAULT_ONLINE_VOICE;
}

/** 把设置里的百分比整数换成 Web Speech 的 rate。 */
export function normalizeVoiceRate(percent: unknown): number {
  const numeric = typeof percent === 'number' && Number.isFinite(percent) ? percent : 100;
  return Math.min(2, Math.max(0.5, Math.round(numeric) / 100));
}

type SpeakOptions = {
  text: string;
  rate: number;
  /** 引擎：在线（默认）或系统语音。 */
  engine?: VoiceEngine;
  /** 在线音色 ShortName。 */
  onlineVoice?: string;
  /** 语速百分比整数，供在线 SSML 使用（rate 是倍率）。 */
  ratePercent?: number;
  volume?: number;
};

/**
 * 朗读结果：`engine` 是真正出声的那条通道，`false` 表示两条都失败。
 * 界面用它如实告诉用户「在线失败、已改用系统语音」，而不是静默降级。
 */
export type SpeakResult = { ok: boolean; engine: VoiceEngine | null };

/**
 * 朗读一句中文。
 *
 * 优先走 Edge 在线语音；任何一步失败都回落到系统 Web Speech API，
 * 再不行就返回 ok=false（绝不抛错影响日程提醒的其它环节）。
 */
export async function speakTimelineTask(options: SpeakOptions): Promise<SpeakResult> {
  if (typeof window === 'undefined') return { ok: false, engine: null };

  const engine = normalizeVoiceEngine(options.engine);

  if (engine === 'online') {
    try {
      const { synthesizeOnlineSpeech, playAudioBlob } = await import('./edgeTts');
      const blob = await synthesizeOnlineSpeech({
        text: options.text,
        voice: normalizeOnlineVoice(options.onlineVoice),
        ratePercent: options.ratePercent ?? Math.round(options.rate * 100),
      });
      playAudioBlob(blob, options.volume ?? 1);
      return { ok: true, engine: 'online' };
    } catch {
      // 在线通道失败（断网 / 代理拦掉 WebSocket）→ 回落系统语音。
    }
  }

  const spoke = await speakWithSystemVoice(options);
  return { ok: spoke, engine: spoke ? 'system' : null };
}

/**
 * 系统语音通道：调用 Rust 后端的 Windows SAPI。
 *
 * 不再使用浏览器的 `speechSynthesis`：实测在 WebView2 里
 * `getVoices()` 返回空数组、但仍会触发 `end`，表现就是「点了没声音也不报错」。
 * 后端 SAPI 与系统「讲述人」同一条通路，只要系统能出声就一定能念。
 */
async function speakWithSystemVoice(options: SpeakOptions): Promise<boolean> {
  try {
    const ratePercent = options.ratePercent ?? Math.round(options.rate * 100);
    // 后端用本地 Piper 神经网络 TTS 合成 WAV 字节（离线、不联网），
    // 播放走这里 —— 与白噪音同一条已验证可用的音频通道。
    // 历史教训：浏览器 speechSynthesis 在 WebView2 里枚举不到语音；
    // Rust 进程内直接朗读时，应用进程的音频到不了音箱。合成与播放分离才可靠。
    const audio = await invokeCommand<{ bytes: number[]; mime_type: string }>(
      'synthesize_timeline_speech',
      { text: options.text, ratePercent },
    );
    diagLog('synthesize result', {
      hasAudio: Boolean(audio),
      byteLength: audio?.bytes?.length ?? 0,
      mime: audio?.mime_type ?? '',
    });
    if (!audio || !Array.isArray(audio.bytes) || audio.bytes.length === 0) {
      diagLog('synthesize returned empty', null);
      return false;
    }
    const blob = new Blob([new Uint8Array(audio.bytes)], { type: audio.mime_type || 'audio/wav' });
    const url = URL.createObjectURL(blob);
    const element = new Audio(url);
    element.volume = Math.min(1, Math.max(0, options.volume ?? 1));
    // 播放结束/失败后释放 Blob URL，避免长时间运行累积内存。
    const release = () => URL.revokeObjectURL(url);
    element.addEventListener('ended', release, { once: true });
    element.addEventListener('error', release, { once: true });
    try {
      await element.play();
      diagLog('play ok', { duration: element.duration, volume: element.volume });
      return true;
    } catch (playError) {
      diagLog('play rejected', {
        message: playError instanceof Error ? playError.message : String(playError),
        blobSize: blob.size,
        mime: blob.type,
      });
      return false;
    }
  } catch (outerError) {
    diagLog('system voice failed', {
      message: outerError instanceof Error ? outerError.message : String(outerError),
    });
    return false;
  }
}

/**
 * 临时诊断：把播报链路的每一步写到 localStorage，便于在不看控制台的情况下定位问题。
 * 只保留最近 20 条，不影响业务。
 */
function diagLog(step: string, detail: unknown) {
  try {
    const key = 'kaoyan:tts-diag';
    const list = JSON.parse(localStorage.getItem(key) ?? '[]') as unknown[];
    list.push({ at: new Date().toISOString(), step, detail });
    localStorage.setItem(key, JSON.stringify(list.slice(-20)));
  } catch {
    // 诊断本身不能影响播报。
  }
}

/** 读取诊断记录（设置页兜底排查用）。 */
export function readTtsDiagnostics(): string {
  try {
    return localStorage.getItem('kaoyan:tts-diag') ?? '[]';
  } catch {
    return '[]';
  }
}

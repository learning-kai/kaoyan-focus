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
  const start = formatMinute(args.startMinute);
  const end = formatMinute(args.endMinute);
  const duration = Math.max(0, args.endMinute - args.startMinute);
  const durationText = duration > 0 ? `，预计 ${formatDurationText(duration)}` : '';
  return `一分钟后开始：${subjectPrefix}${title}。时间 ${start} 到 ${end}${durationText}，请准备。`;
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

/** 把距 00:00 的分钟数转成「上午 8 点 30 分」这种口语说法。 */
export function formatSpokenMinute(minute: number): string {
  const safeMinute = Math.max(0, Math.min(1439, Math.floor(minute)));
  const hour = Math.floor(safeMinute / 60);
  const minutes = safeMinute % 60;
  const period = hour < 12 ? '上午' : hour < 18 ? '下午' : '晚上';
  const hour12 = hour % 12 === 0 ? 12 : hour % 12;
  return minutes === 0 ? `${period}${hour12}点整` : `${period}${hour12}点${minutes}分`;
}

function formatMinute(minute: number) {
  const safeMinute = Math.max(0, Math.min(1439, Math.floor(minute)));
  return `${String(Math.floor(safeMinute / 60)).padStart(2, '0')}:${String(safeMinute % 60).padStart(2, '0')}`;
}

function formatDurationText(minutes: number) {
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} 小时` : `${hours} 小时 ${rest} 分钟`;
}

/** 「自动 / 在线默认音色」的取值：由后端推荐的晓晓。 */
export const DEFAULT_ONLINE_VOICE = 'zh-CN-XiaoxiaoNeural';

/** 播报引擎：在线（默认）或系统语音。 */
export type VoiceEngine = 'online' | 'system';

/**
 * 归一化播报引擎。
 *
 * 默认取 `system`：Edge 在线语音依赖 WSS，开启系统代理（Clash / v2ray 等）后
 * WebSocket 升级经常被拦，表现为「点了试听没有声音」。系统语音不依赖网络，
 * 保证提醒一定能出声；想要更自然的音色可以在设置里主动切到在线。
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
    await invokeCommand('speak_timeline_task', {
      text: options.text,
      ratePercent,
    });
    return true;
  } catch {
    return false;
  }
}

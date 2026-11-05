//! 时间轴任务语音播报（Windows SAPI，进程内 COM）。
//!
//! ## 为什么不用浏览器 speechSynthesis
//!
//! 实测在 WebView2 里 `getVoices()` 返回空数组，却仍会立刻触发 `end` 事件：
//! 表现为「点了试听没有声音，也不报错」。系统明明装了中文语音（Huihui 等）。
//!
//! ## 为什么不用 PowerShell 调 System.Speech
//!
//! 第一版确实是这么做的，结果有两个致命问题：
//! 1. 每次播报都会**弹出一个 PowerShell 控制台窗口**；
//! 2. `Speak()` 是同步阻塞的，而 `Command::output()` 又要等进程退出，
//!    整句念完之前 Tauri 的 IPC 线程一直被占住 —— 界面直接卡死。
//!
//! 所以改成在进程内直接调用 SAPI 的 COM 接口（`ISpVoice`）。
//! 播报放到独立线程执行，命令立即返回，界面不阻塞、也不会有任何外部窗口。

use serde::Serialize;
use std::thread;

/// 允许的语速区间（百分比整数），与设置页保持一致。
const RATE_MIN: i64 = 50;
const RATE_MAX: i64 = 200;
/// 单次播报文本上限，防止把整篇笔记念成有声书。
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct TimelineSpeechStatus {
    /// 是否已发起播报（真正的朗读在后台线程进行）。
    pub spoken: bool,
    /// 实际使用的语音名；拿不到时为空。
    pub voice: String,
    /// 给界面看的一句话结果说明。
    pub detail: String,
}

/// 把设置里的语速百分比换算成 SAPI 的 Rate（-10..10，0 为正常语速）。
///
/// SAPI 的 Rate 是相对档位而不是线性倍率，这里分段映射保证端点精确落在 ±10：
/// 50% → -10，100% → 0，200% → +10。
pub fn percent_to_sapi_rate(percent: i64) -> i32 {
    let clamped = percent.clamp(RATE_MIN, RATE_MAX);
    let rate = if clamped <= 100 {
        ((clamped - 100) as f64 / 50.0 * 10.0).round() as i32
    } else {
        ((clamped - 100) as f64 / 100.0 * 10.0).round() as i32
    };
    rate.clamp(-10, 10)
}

/// 清洗待朗读文本：换行会让 SAPI 停顿异常，统一压成空格；控制字符直接丢弃。
pub fn sanitize_speech_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            // SAPI 对连续标点敏感，保留常规标点即可。
            _ if ch.is_control() => ' ',
            _ => ch,
        })
        .collect();
    cleaned.trim().chars().take(MAX_TEXT_CHARS).collect()
}

/// 在**当前线程**初始化 COM 并同步朗读完。
///
/// 必须每个调用线程各自 `CoInitializeEx`：SAPI 是 apartment-threaded 的 COM 对象，
/// 跨线程共用会拿到 RPC_E_WRONG_THREAD。
#[cfg(windows)]
fn speak_blocking(text: &str, rate: i32, volume: u32) -> Result<String, String> {
    use windows::core::HSTRING;
    use windows::Win32::Media::Speech::ISpVoice;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    // 语音对象 CLSID：SpVoice {96749377-3391-11D2-9EE3-00C04F797396}
    const CLSID_SP_VOICE: windows::core::GUID =
        windows::core::GUID::from_u128(0x96749377_3391_11d2_9ee3_00c04f797396);

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // RPC_E_CHANGED_MODE 说明该线程已被其它方式初始化，SAPI 仍可用，继续。
        if hr.is_err() && hr.0 as u32 != 0x8001_0106 {
            return Err(format!("COM 初始化失败：{hr}"));
        }

        // 无论后面成不成功，都要配对 CoUninitialize。
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _guard = ComGuard;

        let voice: ISpVoice = CoCreateInstance(&CLSID_SP_VOICE, None, CLSCTX_ALL)
            .map_err(|error| format!("创建语音对象失败：{error}"))?;

        voice
            .SetRate(rate)
            .map_err(|error| format!("设置语速失败：{error}"))?;
        voice
            .SetVolume(volume as u16)
            .map_err(|error| format!("设置音量失败：{error}"))?;

        // 用系统默认语音。中文版 Windows 的默认语音即 Huihui，
        // 在缺少显式语音选择 API 的情况下这是最稳的做法（见 GetVoice 返回 ISpObjectToken）。

        let text = HSTRING::from(text);
        // SPF_DEFAULT = 0，同步朗读；本函数已在独立线程里跑，不会阻塞界面。
        voice
            .Speak(&text, 0, None)
            .map_err(|error| format!("朗读失败：{error}"))?;

        Ok(String::new())
    }
}

/// 供前端「试听播报」和定时播报共用的命令。
///
/// **立即返回**，真正的朗读在后台线程进行 —— 念一整句话要好几秒，
/// 同步等待会让界面卡死（这正是第一版 PowerShell 方案的真实故障）。
#[tauri::command]
pub fn speak_timeline_task(
    text: String,
    rate_percent: Option<i64>,
) -> Result<TimelineSpeechStatus, String> {
    let cleaned = sanitize_speech_text(&text);
    if cleaned.is_empty() {
        return Err("播报内容为空".to_string());
    }
    let rate = percent_to_sapi_rate(rate_percent.unwrap_or(100));

    thread::Builder::new()
        .name("timeline-tts".to_string())
        .spawn(move || {
            if let Err(error) = speak_blocking(&cleaned, rate, 100) {
                // 后台线程没有回传通道，错误只能进日志；调用方已成功拿到「已发起」。
                eprintln!("[timeline_tts] {error}");
            }
        })
        .map_err(|error| format!("无法启动播报线程：{error}"))?;

    Ok(TimelineSpeechStatus {
        spoken: true,
        voice: String::new(),
        detail: "已发起系统语音播报".to_string(),
    })
}

#[cfg(not(windows))]
fn speak_blocking(_text: &str, _rate: i32, _volume: u32) -> Result<String, String> {
    Err("当前系统不支持系统语音播报".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_sapi_rate_maps_endpoints() {
        assert_eq!(percent_to_sapi_rate(100), 0, "正常语速应对应 SAPI 的 0");
        assert_eq!(percent_to_sapi_rate(50), -10, "最慢档对应 50%");
        assert_eq!(percent_to_sapi_rate(200), 10, "最快档对应 200%");
    }

    #[test]
    fn percent_to_sapi_rate_clamps_out_of_range() {
        assert_eq!(percent_to_sapi_rate(10), -10, "过低应钳到下限");
        assert_eq!(percent_to_sapi_rate(999), 10, "过高应钳到上限");
    }

    #[test]
    fn sanitize_removes_line_breaks() {
        assert_eq!(sanitize_speech_text("一分钟后\n开始"), "一分钟后 开始");
        assert_eq!(sanitize_speech_text("  空白  "), "空白");
    }

    #[test]
    fn sanitize_limits_length() {
        let long = "字".repeat(MAX_TEXT_CHARS + 100);
        assert_eq!(sanitize_speech_text(&long).chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn empty_text_is_rejected() {
        let result = speak_timeline_task("   ".to_string(), Some(100));
        assert!(result.is_err(), "空白内容不应发起播报");
    }
}

#[cfg(all(test, windows))]
mod windows_smoke {
    use super::*;

    /// 真机冒烟：确认 SAPI 在进程内能正常创建并朗读（会真的出声）。
    /// 用 `cargo test timeline_tts_smoke -- --ignored --nocapture` 手动触发。
    #[test]
    #[ignore]
    fn timeline_tts_smoke_speaks() {
        let result = speak_blocking("语音播报测试", 0, 100);
        assert!(result.is_ok(), "SAPI 朗读失败：{result:?}");
    }
}

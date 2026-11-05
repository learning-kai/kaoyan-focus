//! 时间轴任务语音播报（Windows SAPI 合成 → 前端音频通道播放）。
//!
//! ## 走过的两条弯路
//!
//! 1. **浏览器 `speechSynthesis`**：WebView2 里 `getVoices()` 返回空数组，
//!    却仍立刻触发 `end` —— 没声音也不报错。
//! 2. **Rust 进程内 COM `ISpVoice` 直接朗读**：`Speak()` 确实执行了（耗时 3 秒以上，
//!    说明真的在念），但应用进程的音频会话没有送到用户音箱。
//!    同一台机器上白噪音（走 WebView2 音频）却能正常听到。
//!
//! ## 现在的做法
//!
//! **Rust 只负责合成，播放交给前端**：
//! - Rust 用 SAPI 把文本合成成 WAV 字节返回（不朗读，不碰音频设备）；
//! - 前端拿到字节后用 `<audio>` / `Audio()` 播放 —— 与白噪音同一条已验证可用的通道。
//!
//! 这样既绕开了浏览器语音引擎的缺陷，也绕开了「Rust 进程音频不可达」的问题。

use serde::Serialize;

/// 允许的语速区间（百分比整数），与设置页保持一致。
const RATE_MIN: i64 = 50;
const RATE_MAX: i64 = 200;
/// 单次合成文本上限，防止把整篇笔记念成有声书。
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct TimelineSpeechAudio {
    /// WAV 文件的字节内容（含 44 字节 RIFF 头）。
    pub bytes: Vec<u8>,
    /// MIME 类型，前端构造 Blob 时直接用。
    pub mime_type: String,
}

/// 把设置里的语速百分比换算成 SAPI 的 Rate（-10..10，0 为正常语速）。
///
/// SAPI 的 Rate 是相对档位而不是线性倍率，分段映射保证端点精确落在 ±10：
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

/// 清洗待合成文本：换行压成空格，控制字符丢弃。
pub fn sanitize_speech_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            _ if ch.is_control() => ' ',
            _ => ch,
        })
        .collect();
    cleaned.trim().chars().take(MAX_TEXT_CHARS).collect()
}

/// 用 SAPI 合成 WAV 字节。
///
/// 走 PowerShell 调 `System.Speech` 的 `SetOutputToWaveFile`：
/// 它只做离线合成、不碰音频设备，因此不会再出现「声音送不到音箱」的问题，
/// 也不会像直接 `Speak()` 那样占住调用线程几秒钟。
#[cfg(windows)]
fn synthesize_wav(text: &str, rate: i32) -> Result<Vec<u8>, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // 输出到临时文件：SAPI 的 WaveFile 输出需要真实路径。
    let temp_dir = std::env::temp_dir();
    let file_name = format!(
        "kaoyan-tts-{}-{}.wav",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let wav_path = temp_dir.join(&file_name);

    // 脚本固定，文本通过环境变量传入，避免任何引号/换行转义问题。
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$path = $env:KAOYAN_TTS_WAV
$rate = [int]$env:KAOYAN_TTS_RATE
$text = $env:KAOYAN_TTS_TEXT
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
try {
  $synth.Rate = $rate
  $zh = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'zh-*' } | Select-Object -First 1
  if ($zh) { $synth.SelectVoice($zh.VoiceInfo.Name) }
  $synth.SetOutputToWaveFile($path)
  $synth.Speak($text)
} finally {
  $synth.Dispose()
}
"#;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("KAOYAN_TTS_WAV", &wav_path)
        .env("KAOYAN_TTS_RATE", rate.to_string())
        .env("KAOYAN_TTS_TEXT", text)
        .stdin(Stdio::null())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW：绝不弹出控制台窗口
        .output()
        .map_err(|error| format!("调用系统语音合成失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&wav_path);
        return Err(format!("语音合成失败：{}", stderr.trim()));
    }

    let bytes = std::fs::read(&wav_path).map_err(|error| format!("读取合成音频失败：{error}"))?;
    let _ = std::fs::remove_file(&wav_path);

    if bytes.len() <= 44 {
        return Err("语音合成结果为空".to_string());
    }
    Ok(bytes)
}

/// 合成一段用于「开始前 1 分钟」播报的中文语音，返回 WAV 字节交给前端播放。
#[tauri::command]
pub fn synthesize_timeline_speech(
    text: String,
    rate_percent: Option<i64>,
) -> Result<TimelineSpeechAudio, String> {
    let cleaned = sanitize_speech_text(&text);
    if cleaned.is_empty() {
        return Err("播报内容为空".to_string());
    }
    let rate = percent_to_sapi_rate(rate_percent.unwrap_or(100));
    let bytes = synthesize_wav(&cleaned, rate)?;
    Ok(TimelineSpeechAudio {
        bytes,
        mime_type: "audio/wav".to_string(),
    })
}

#[cfg(not(windows))]
fn synthesize_wav(_text: &str, _rate: i32) -> Result<Vec<u8>, String> {
    Err("当前系统不支持系统语音合成".to_string())
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
        assert!(
            synthesize_timeline_speech("   ".to_string(), Some(100)).is_err(),
            "空白内容不应发起合成"
        );
    }
}

#[cfg(all(test, windows))]
mod windows_smoke {
    use super::*;

    /// 真机冒烟：确认能合成出合法的 WAV（不依赖音频设备，可稳定断言）。
    /// 手动跑：`cargo test timeline_tts_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn timeline_tts_smoke_synthesizes_valid_wav() {
        let audio = synthesize_timeline_speech("语音播报测试".to_string(), Some(100))
            .expect("应能合成语音");
        assert_eq!(audio.mime_type, "audio/wav");
        assert!(audio.bytes.len() > 44, "应大于 WAV 头长度");
        assert_eq!(&audio.bytes[0..4], b"RIFF", "应是合法 RIFF 头");
        assert_eq!(&audio.bytes[8..12], b"WAVE", "应是合法 WAVE 格式");
    }
}

//! 时间轴任务语音播报（Windows SAPI）。
//!
//! 为什么不用浏览器 speechSynthesis：
//! 实测在 WebView2 / 部分 Chromium 环境下 `speechSynthesis.getVoices()` 返回空数组，
//! 但仍然会立刻触发 `end` 事件 —— 表现为「点了试听完全没有声音，也不报错」。
//! 系统明明装了中文语音（Huihui / Kangkang / Yaoyao），却一句也念不出来。
//!
//! 所以播报改由 Rust 直接调用 Windows 自带的 SAPI：
//! 走 `System.Speech` 的 SpeechSynthesizer，与系统「讲述人」同一条通路，
//! 不依赖浏览器、不依赖网络、也不受代理影响。
//!
//! 实现方式：调 PowerShell 执行一段极简脚本，把文本作为参数传入。
//! 相比在 Rust 里手写 COM（需要额外 windows crate feature 与大量 unsafe），
//! 这条路只依赖系统自带组件，行为可通过脚本单独验证，出错也容易复现。

use serde::Serialize;
use std::process::{Command, Stdio};

/// 允许的语速区间（百分比整数），与设置页保持一致。
const RATE_MIN: i64 = 50;
const RATE_MAX: i64 = 200;
/// 单次播报文本上限，防止把整篇笔记念成有声书。
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct TimelineSpeechStatus {
    /// 是否成功发起播报。
    pub spoken: bool,
    /// 实际使用的语音名（取系统默认中文语音时可能为空）。
    pub voice: String,
    /// 给界面看的一句话结果说明。
    pub detail: String,
}

/// 把设置里的语速百分比换算成 SAPI 的 Rate（-10..10，0 为正常语速）。
///
/// SAPI 的 Rate 是相对档位而不是线性倍率，这里按常用区间线性映射：
/// 100% → 0，50% → -10，200% → +10，并四舍五入到整数。
pub fn percent_to_sapi_rate(percent: i64) -> i32 {
    let clamped = percent.clamp(RATE_MIN, RATE_MAX);
    // 50% → -10（最慢），100% → 0，200% → +10（最快）。
    // 上下半区跨度不同（50→100 是 50 个百分点，100→200 是 100 个百分点），
    // 分段映射保证端点精确落在 ±10。
    let rate = if clamped <= 100 {
        ((clamped - 100) as f64 / 50.0 * 10.0).round() as i32
    } else {
        ((clamped - 100) as f64 / 100.0 * 10.0).round() as i32
    };
    rate.clamp(-10, 10)
}

/// 清洗待朗读文本：去掉会破坏 PowerShell 单引号字符串与换行的字符。
pub fn sanitize_speech_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            '\'' => '，',
            _ => ch,
        })
        .collect();
    cleaned.trim().chars().take(MAX_TEXT_CHARS).collect()
}

/// 构造 PowerShell 脚本。
///
/// 文本通过参数传入而不是拼进脚本字符串，避免转义地狱；
/// 脚本本身固定，只把 `-Text` / `-Rate` 当参数收。
pub fn build_speech_script() -> &'static str {
    r#"
param([string]$Text, [int]$Rate)
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
try {
  $synth.Rate = $Rate
  $synth.Volume = 100
  # 优先挑中文语音；没有就用系统默认，绝不因为找不到中文而静默失败。
  $zh = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -like 'zh-*' } | Select-Object -First 1
  if ($zh) { $synth.SelectVoice($zh.VoiceInfo.Name) }
  $synth.Speak($Text)
  Write-Output ('VOICE=' + $synth.Voice.Name)
} catch {
  Write-Output ('ERROR=' + $_.Exception.Message)
  exit 1
} finally {
  $synth.Dispose()
}
"#
}

/// 播报一句中文。返回是否成功，以及实际使用的语音名。
pub fn speak_with_sapi(text: &str, rate_percent: i64) -> Result<TimelineSpeechStatus, String> {
    let cleaned = sanitize_speech_text(text);
    if cleaned.is_empty() {
        return Err("播报内容为空".to_string());
    }

    let script = build_speech_script();
    let rate = percent_to_sapi_rate(rate_percent);

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .arg("-Text")
        .arg(&cleaned)
        .arg("-Rate")
        .arg(rate.to_string())
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("调用系统语音失败：{error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let reason = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(format!("系统语音播报失败：{reason}"));
    }

    let voice = stdout
        .lines()
        .find_map(|line| line.strip_prefix("VOICE="))
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(TimelineSpeechStatus {
        spoken: true,
        voice,
        detail: "已用系统语音播报".to_string(),
    })
}

/// 供前端「试听播报」和定时播报共用的命令。
#[tauri::command]
pub fn speak_timeline_task(
    text: String,
    rate_percent: Option<i64>,
) -> Result<TimelineSpeechStatus, String> {
    speak_with_sapi(&text, rate_percent.unwrap_or(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_sapi_rate_maps_endpoints() {
        assert_eq!(percent_to_sapi_rate(100), 0, "正常语速应对应 SAPI 的 0");
        assert_eq!(
            percent_to_sapi_rate(50),
            -10,
            "最慢档 (-10) 在设置里对应 50%"
        );
        assert_eq!(percent_to_sapi_rate(200), 10, "最快档对应 200%");
    }

    #[test]
    fn percent_to_sapi_rate_clamps_out_of_range() {
        assert_eq!(percent_to_sapi_rate(10), -10, "过低应钳到下限");
        assert_eq!(percent_to_sapi_rate(999), 10, "过高应钳到上限");
    }

    #[test]
    fn sanitize_removes_line_breaks_and_quotes() {
        assert_eq!(sanitize_speech_text("一分钟后\n开始"), "一分钟后 开始");
        assert_eq!(sanitize_speech_text("it's ok"), "it，s ok");
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
            speak_with_sapi("   ", 100).is_err(),
            "空白内容不应调用系统语音"
        );
    }

    #[test]
    fn script_receives_text_as_parameter_not_interpolated() {
        let script = build_speech_script();
        // 文本必须走参数，避免引号/分号把脚本注入坏掉。
        assert!(
            script.contains("param([string]$Text"),
            "脚本应以参数接收文本"
        );
        assert!(script.contains("$synth.Speak($Text)"), "应朗读传入的文本");
        assert!(!script.contains("Speak('"), "不应把文本硬拼进脚本");
    }
}

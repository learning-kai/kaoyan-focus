//! 时间轴任务语音播报（本地神经网络 TTS：Piper）。
//!
//! ## 为什么不用系统 SAPI
//!
//! Windows 自带的 SAPI 语音（Huihui / Kangkang）是十几年前的拼接式 TTS，
//! 机械感明显，而且必须先装好语音包、再依赖注册表里的 token。
//! 它只能保证「有声音」，谈不上「听着舒服」。
//!
//! ## 现在用什么
//!
//! [Piper](https://github.com/rhasspy/piper) —— 纯本地运行的神经网络 TTS：
//! - 完全离线，不联网、不依赖系统语音包、不依赖浏览器
//! - 中文音色 `zh_CN-huayan-medium`（女声，22.05kHz），音质远超 SAPI
//! - 单句合成约 0.1–0.2 秒（实时率 ~0.05），比播放快十几倍
//!
//! 运行时（piper.exe + onnxruntime + espeak-ng-data）与语音模型随安装包一起分发
//! （见 `tauri.conf.json` 的 `bundle.resources`），用户装完即可离线使用。
//!
//! ## 播放为什么在前端
//!
//! Rust 进程内的音频会话在这台机器上到不了音箱（实测 `ISpVoice.Speak` 返回 Ok、
//! 耗时也正常，就是没声）；而 WebView 里的 `Audio` 播放（白噪音走的同一条路）
//! 是验证过能出声的。所以后端只负责**合成**，播放交给前端。

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 允许的语速区间（百分比整数），与设置页保持一致。
const RATE_MIN: i64 = 50;
const RATE_MAX: i64 = 200;
/// 单次合成文本上限，防止把整篇笔记念成有声书。
const MAX_TEXT_CHARS: usize = 500;
/// 随包分发的中文语音模型。
const VOICE_MODEL: &str = "zh_CN-huayan-medium.onnx";

#[derive(Debug, Clone, Serialize)]
pub struct TimelineSpeechAudio {
    /// WAV 文件字节（含 44 字节 RIFF 头）。
    pub bytes: Vec<u8>,
    /// MIME 类型，前端构造 Blob 时直接用。
    pub mime_type: String,
    /// 实际使用的语音模型名，便于界面展示。
    pub voice: String,
    /// 合成引擎标识。
    pub engine: String,
}

/// 把百分比语速换算成 Piper 的 `--length_scale`。
///
/// Piper 用的是**时长缩放**（数值越小念得越快），和百分比方向相反：
/// 100% → 1.0，200% → 0.5（快一倍），50% → 2.0（慢一倍）。
pub fn percent_to_length_scale(percent: i64) -> f64 {
    let clamped = percent.clamp(RATE_MIN, RATE_MAX);
    let scale = 100.0 / clamped as f64;
    // 留出合理边界，避免极端值把合成拖到几十秒。
    (scale.clamp(0.4, 2.5) * 100.0).round() / 100.0
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

/// 定位随包分发的 Piper 目录。
///
/// 开发期在 `src-tauri/resources/piper`；打包后由 Tauri 解到资源目录。
/// 两条路径都试，找到哪个用哪个。
pub fn resolve_piper_dir(app_data_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1) 打包后的资源目录（Tauri 会把 resources 平铺到 exe 同级的 resources/ 下）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("resources").join("piper"));
            candidates.push(dir.join("_up_").join("resources").join("piper"));
        }
    }
    // 2) 运行时数据目录（用户可手动放置覆盖）
    if let Some(dir) = app_data_dir {
        candidates.push(dir.join("piper"));
    }
    // 3) 开发期源码目录
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("piper"),
    );

    candidates.into_iter().find(|dir| {
        dir.join("piper.exe").is_file()
                && dir.join(VOICE_MODEL).is_file()
                // espeak-ng 的音素数据是必需的：缺了 `voices`/`lang` 目录时
                // piper 会「加载成功、合成 0 字节、退出码 127」，极难排查。
                && dir.join("espeak-ng-data").join("voices").is_dir()
                && dir.join("espeak-ng-data").join("lang").is_dir()
                && dir.join("espeak-ng-data").join("phontab").is_file()
    })
}

/// 把 Piper 运行时准备到一个**纯 ASCII 路径**下，返回该目录。
///
/// Piper 内置的 espeak-ng 用 `fopen` 读 `espeak-ng-data/phontab`，
/// 路径里只要有非 ASCII 字符（例如 `E:\大学\大三\考研专注\...`）就会报
/// `Illegal byte sequence` 然后直接退出。用户把应用装在中文目录下很常见，
/// 所以这里先把运行时镜像到 `%LOCALAPPDATA%\kaoyan-focus\tts\` 再调用。
///
/// 只在首次或版本变化时复制，之后复用；判断依据是 piper.exe 的大小与时间戳。
fn ensure_ascii_runtime(source_dir: &Path) -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 LOCALAPPDATA".to_string())?
        .join("kaoyan-focus")
        .join("tts")
        .join("piper");

    let source_exe = source_dir.join("piper.exe");
    let need_copy = match (
        std::fs::metadata(&source_exe),
        std::fs::metadata(base.join("piper.exe")),
    ) {
        (Ok(src), Ok(dst)) => src.len() != dst.len() || src.modified().ok() != dst.modified().ok(),
        (Ok(_), Err(_)) => true,
        (Err(error), _) => return Err(format!("Piper 运行时缺失：{error}")),
    };

    if need_copy {
        // 先清空旧目录，避免残留旧版本 dll 混在一起。
        let _ = std::fs::remove_dir_all(&base);
        copy_dir_recursive(source_dir, &base)?;
    }

    Ok(base)
}

/// 递归复制目录（Tauri 没有现成的公共 API，这里手写，够用即可）。
fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("创建目录失败：{error}"))?;
    let entries = std::fs::read_dir(from).map_err(|error| format!("读取目录失败：{error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &target)?;
        } else {
            std::fs::copy(&source, &target)
                .map_err(|error| format!("复制 {} 失败：{error}", source.display()))?;
        }
    }
    Ok(())
}

/// 用 Piper 合成 WAV 字节。纯本地推理，不联网。
fn synthesize_with_piper(
    piper_dir: &Path,
    text: &str,
    length_scale: f64,
) -> Result<Vec<u8>, String> {
    use std::os::windows::process::CommandExt;

    let exe = piper_dir.join("piper.exe");
    let model = piper_dir.join(VOICE_MODEL);
    let wav_path = std::env::temp_dir().join(format!(
        "kaoyan-piper-{}-{}.wav",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));

    // 文本走 stdin，彻底避开命令行转义问题（含引号、中文、标点都安全）。
    let mut child = Command::new(&exe)
        .current_dir(piper_dir)
        .args([
            "--model",
            model.to_string_lossy().as_ref(),
            "--output_file",
            wav_path.to_string_lossy().as_ref(),
            "--length_scale",
            &format!("{length_scale:.2}"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW：不弹控制台
        .spawn()
        .map_err(|error| format!("启动语音合成引擎失败：{error}"))?;

    {
        use std::io::Write;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "无法写入语音合成输入".to_string())?;
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| format!("写入合成文本失败：{error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("语音合成进程异常：{error}"))?;

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
    app: tauri::AppHandle,
    text: String,
    rate_percent: Option<i64>,
) -> Result<TimelineSpeechAudio, String> {
    use tauri::Manager;

    let cleaned = sanitize_speech_text(&text);
    if cleaned.is_empty() {
        return Err("播报内容为空".to_string());
    }

    let app_data = app.path().app_data_dir().ok();
    let piper_dir = resolve_piper_dir(app_data.as_deref()).ok_or_else(|| {
        "未找到本地语音引擎（resources/piper）。请重新安装应用，或把 Piper 放到数据目录的 piper/ 下。"
            .to_string()
    })?;

    // 关键：Piper 的 espeak-ng 无法处理非 ASCII 路径，必须先镜像到纯英文目录。
    let runtime_dir = ensure_ascii_runtime(&piper_dir)?;

    let length_scale = percent_to_length_scale(rate_percent.unwrap_or(100));
    let bytes = synthesize_with_piper(&runtime_dir, &cleaned, length_scale)?;

    Ok(TimelineSpeechAudio {
        bytes,
        mime_type: "audio/wav".to_string(),
        voice: "zh_CN-huayan-medium".to_string(),
        engine: "piper".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_length_scale_is_inverse_of_rate() {
        assert_eq!(percent_to_length_scale(100), 1.0, "正常语速应对应 1.0");
        assert_eq!(percent_to_length_scale(200), 0.5, "快一倍 = 一半时长");
        assert_eq!(percent_to_length_scale(50), 2.0, "慢一倍 = 两倍时长");
    }

    #[test]
    fn percent_to_length_scale_clamps() {
        assert_eq!(percent_to_length_scale(10), 2.0, "过低应钳到最慢");
        assert_eq!(percent_to_length_scale(999), 0.5, "过高应钳到最快");
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
            sanitize_speech_text("   ").is_empty(),
            "空白文本应被识别成空"
        );
    }

    #[test]
    fn espeak_data_has_phoneme_sources() {
        // 精简 espeak 数据时曾误删 voices/lang，导致 piper 静默产出 0 字节音频。
        // 这里锁住这几个必需项，避免同类问题再次漏过。
        let dir = resolve_piper_dir(None).expect("应找到 piper 目录");
        let espeak = dir.join("espeak-ng-data");
        assert!(espeak.join("voices").is_dir(), "缺少 espeak-ng-data/voices");
        assert!(espeak.join("lang").is_dir(), "缺少 espeak-ng-data/lang");
        assert!(
            espeak.join("phontab").is_file(),
            "缺少 espeak-ng-data/phontab"
        );
        assert!(
            espeak.join("phondata").is_file(),
            "缺少 espeak-ng-data/phondata"
        );
        assert!(espeak.join("cmn_dict").is_file(), "缺少中文词典 cmn_dict");
    }

    #[test]
    fn piper_dir_resolves_in_development() {
        // 开发期 resources/piper 一定存在（随仓库分发）。
        let dir = resolve_piper_dir(None);
        assert!(dir.is_some(), "开发期应能找到 resources/piper");
        let dir = dir.unwrap();
        assert!(dir.join("piper.exe").is_file(), "应有 piper.exe");
        assert!(dir.join(VOICE_MODEL).is_file(), "应有中文语音模型");
    }
}

#[cfg(all(test, windows))]
mod piper_smoke {
    use super::*;

    /// 真机冒烟：用随包引擎合成一句中文，确认输出是合法且有内容的 WAV。
    /// 手动跑：`cargo test piper_tts_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn piper_tts_smoke_synthesizes_real_audio() {
        let dir = resolve_piper_dir(None).expect("应找到 piper 目录");
        let runtime = ensure_ascii_runtime(&dir).expect("应能准备 ASCII 运行时目录");
        let bytes = synthesize_with_piper(&runtime, "一分钟后开始，数学套卷训练", 1.0)
            .expect("应能合成语音");
        assert!(
            bytes.len() > 2000,
            "合成结果过小（{} 字节），说明引擎没有真正产出音频",
            bytes.len()
        );
        assert!(bytes.len() > 44, "应大于 WAV 头长度");
        assert_eq!(&bytes[0..4], b"RIFF", "应是合法 RIFF 头");
        assert_eq!(&bytes[8..12], b"WAVE", "应是合法 WAVE 格式");

        // 抽样检查不是静音：任意 16-bit 样本明显偏离 0 即认为有声音。
        let peak = bytes[44..]
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(peak > 1000, "合成结果应有实际音量，实测峰值 {peak}");
    }
}

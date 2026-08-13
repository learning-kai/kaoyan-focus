from pathlib import Path

# FEATURES.md
features = Path(r"E:\BaiduSyncdisk\大学\大三\大三下\考研专注\考研专注\FEATURES.md")
text = features.read_text(encoding="utf-8")
old = """### 2.7 声音与桌面提醒

以下真实阶段变化会播放一声提示音，并在 Windows 右下角显示桌面通知：

- 学习模式开始。
- 番茄钟到点并进入等待休息确认。
- 本人确认开始休息。
- 休息结束并进入下一轮番茄钟。
- 学习模式自然完成。

切换页面后再次进入专注页不会触发通知，通知只绑定真实阶段变化。
"""
new = """### 2.7 声音与桌面提醒

以下真实阶段变化会播放一声提示音，并在 Windows 右下角显示桌面通知：

- 学习模式开始。
- 番茄钟到点并进入等待休息确认。
- 本人确认开始休息。
- 休息结束并进入下一轮番茄钟。
- 学习模式自然完成。
- 日历事项到达提前提醒窗口（默认提前 5 分钟，可在设置中开关）。

设置页提供「日历铃声」开关（`schedule_reminder_enabled`）与提前量（`schedule_reminder_lead_minutes`，默认 5 分钟）。关闭后日历不再响铃，仍保留闹钟和专注阶段提醒。

切换页面后再次进入专注页不会触发通知，通知只绑定真实阶段变化。
"""
if old not in text:
    raise SystemExit('FEATURES block missing')
features.write_text(text.replace(old, new), encoding='utf-8')
print('FEATURES updated')

# CHANGELOG
changelog = Path(r"E:\BaiduSyncdisk\大学\大三\大三下\考研专注\考研专注\CHANGELOG.md")
ctext = changelog.read_text(encoding="utf-8")
marker = "### Changed\n"
insert = """### Changed

- Clarified the calendar ringtone setting as 「日历铃声」 in Settings, with the existing schedule-reminder switch also exposed under the sound tab for easier discovery.
"""
# Avoid double insert
if "日历铃声" in ctext and "sound tab for easier discovery" in ctext:
    print('CHANGELOG already has entry')
else:
    # Insert under Unreleased Changed section
    idx = ctext.find(marker)
    if idx < 0:
        raise SystemExit('Changed marker missing')
    # Find the first Changed under Unreleased - the first occurrence after Unreleased
    unreleased = ctext.find("## Unreleased")
    changed = ctext.find(marker, unreleased)
    if changed < 0:
        raise SystemExit('Unreleased Changed missing')
    # Insert a bullet after ### Changed
    insert_at = changed + len(marker)
    bullet = "- Clarified the calendar ringtone setting as 「日历铃声」 in Settings, with the existing schedule-reminder switch also exposed under the sound tab for easier discovery.\n"
    ctext = ctext[:insert_at] + bullet + ctext[insert_at:]
    changelog.write_text(ctext, encoding='utf-8')
    print('CHANGELOG updated')

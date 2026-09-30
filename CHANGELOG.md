# Changelog

All notable changes to `考研专注` will be documented here. The public desktop line follows semantic versioning where practical and focuses on user-visible changes, compatibility notes, security fixes and maintenance work.

## Unreleased

### Added

- Added 随机微休息 (random micro-breaks): during focus a soft cue plays at an unpredictable time within a chosen range (default 3–5 minutes); the user closes their eyes for a few seconds (default 10) until a second cue. An optional focus-algorithm mode keeps cues dense for the first 30 minutes, spaces them out between 30 and 60 minutes, and tightens them again after 60 minutes. Cue timing is decided by the Rust background tick, so it keeps working while the window is hidden in the tray, and each round's schedule is derived deterministically from the study mode id and cycle, so a restart neither replays nor loses cues. Includes a full-screen eyes-closed countdown, an in-session toggle, a one-click 「90/20 专注法」 preset (90-minute focus, 20-minute breaks), new `get_micro_break_settings` / `save_micro_break_settings` commands and `micro_break_*` settings keys. Covered by `cargo test micro_break` and `npm run test:micro-break`.
- Added a 白噪音 ambient sound mixer, opened from the bottom of the sidebar: 14 bundled loops (rain, storm, waves, wind, stream, birds, summer night, train, city, boat, coffee shop, fireplace, pink noise, white noise), any number playing at once with per-sound and master volume, a master pause switch, fade in/out, and playback that survives page switches and minimising to the tray. The mix is restored on restart but stays paused until turned on. Sounds ship in `public/sounds/ambient/` (~24 MB), with attribution in `NOTICE.md` and in the panel. Covered by `npm run test:ambient-sound`.
- Added `miniapp-sync/`, an optional real-time sync channel between the desktop database and a WeChat mini program: a Node relay server (REST + SSE + WebSocket, snapshot + oplog + central conflict arbitration), a dependency-free Python PC agent that scans `kaoyan-focus.sqlite3` and writes whitelisted fields back, and a mini program with 今日 / 清单 / 复盘 / 我的 tabs. The desktop source code is untouched; `settings` and all credential fields are excluded from the sync channel by design.
- Added `miniapp-sync/agent/doctor.py`, a layered connectivity doctor (DNS → TCP → TLS/cert → HTTPS over three proxy routes → large payload → SSE) that reports which layer failed instead of a bare timeout.
- Added `miniapp-sync/agent/tunnel-ssh.cmd`, a Windows keep-alive SSH tunnel script, as a fallback when the local network interrupts TLS to the public sync domain.
- Added explicit transport controls to the PC agent: `server.proxyMode` (`none` / `auto` / `custom`), `server.proxyUrl`, `server.forceIpv4`, `server.retryAttempts`, and a `server.fallback` endpoint with automatic failover and probing.
- Added paginated downlink: `/snapshot` accepts `limit`/`offset` (both the REST API and the WebSocket `hello`/`pull` messages), and `/delta` accepts `limit`. `deltaSince` now returns `nextCursor`, `pending` and `truncated` so clients advance the cursor by the last op sequence instead of jumping to `serverRev`.
- Added `python agent/sync_agent.py --prune-orphans [--type X] [--yes]` to list and clean up entities that exist on the sync server but are no longer known to the local `sync_meta` (preview by default, `--yes` to push tombstones).
- Added `miniapp-sync/scripts/e2e-smoke.sh`, a one-command three-end smoke test (database copy → PC agent → relay server → mini program logic) that never touches the live database or the public server.
- Added real-device connection diagnostics to the mini program. A real phone validates the WSS domain against `socket 合法域名` in the WeChat MP backend while DevTools does not (the `urlCheck` setting only applies to DevTools), so a mis-configured domain left the UI stuck on 「连接中」 with no explanation. Every connect failure path (`fail`, `onError`, `send` failure, watchdog timeout) now records the raw `errMsg`, translates it into an actionable hint, and surfaces it on the 今日 page and in a new 「连接诊断」 card on the 我的 page. `sync.lastErrorInfo()` exposes the same data to code.
- Added a handshake watchdog (`timing.connectWatchdogMs`, default 15s) to the mini program. When WeChat silently blocks a handshake the socket neither opens nor errors, which is what produced the permanent 「连接中」. The watchdog now converts that into a diagnosable failure plus a reconnect.
- Added `miniapp-sync/tests/mini-connect-diagnostic-test.js` (21 assertions, no server required): failure留痕, watchdog fallback, config guard, stale-callback isolation and local-address allowance.
- Added `miniapp-sync/tests/ws_handshake_probe.py`, which replays the mini program's exact handshake (same URL, `Origin: https://servicewechat.com`, phone UA) against the production endpoint and prints the TLS version, certificate chain and HTTP status line, so "服务端问题" and "小程序白名单问题" can be told apart immediately.
- Added a 「真机连不上（模拟器正常）」 section to the sync README covering the whitelist format rules, the ICP filing / TLS / certificate-chain prerequisites, the phone-side 「打开调试」 switch that disables domain checking (the fastest way to confirm the diagnosis), and why 「真机调试」 working does not imply the release build works.

### Changed

- `miniapp-sync` digest scope is now consistent across all three ends: **live entities only, tombstones excluded**. Tombstone `deletedAt` values are per-device local timestamps, so including them made the three ends disagree forever and triggered a pointless forced full re-push every 5 minutes. Drift is still detected, because a one-sided delete shows up as an extra live entity.
- Relay server field writes now **merge** the sanitized incoming fields into the current entity instead of replacing the whole field set. The mini program may only write whitelisted fields (`schema.js`), so replacing made a title edit on the phone silently drop desktop-authored fields such as `checklist_task.column_name`. That left the server disagreeing with both ends, which made the digest flap and degraded new-device snapshots. The stored `hash` is now derived from the merged fields so the entity and its fingerprint always agree.
- Consistency checking is now confirmatory instead of single-shot. The desktop writes an in-flight focus session's `actual_seconds` every second and phone edits land on the server first, so the read window between the local scan and the server query routinely catches a transient difference. Both ends now re-check before escalating: the PC agent retries up to three times (re-scanning locally and pushing any regular diff first) and only then falls back to a forced full re-push, while the mini program re-checks once before pulling a full snapshot. Previously a single unlucky read could trigger a 3000+ entity re-push that also risked overriding a phone edit the desktop had not yet received over SSE.
- The PC agent no longer writes a `sync_meta` touch and `commit()` for entities whose content already matches; that path is hit by every replayed op and previously cost thousands of commits per cycle.
- The PC agent refuses to mint a new `sync_id` for entities that are deactivated and have no `sync_id` yet (`mint_guard`). The desktop's `normalize_default_subjects()` deactivates duplicate subjects and deletes their `sync_meta` rows, so minting for them caused an endless "mint → desktop deletes → mint again" loop that piled duplicate subjects onto the server on every run.
- The PC agent's `statePath` / `logPath` and the test database copy now live outside cloud-synced folders (`%LOCALAPPDATA%` / `%TEMP%`). Writing sync state into a cloud-synced directory triggered an upload per save and stretched a single run to tens of seconds.
- PC agent relative paths are now resolved against the config file's directory rather than the current working directory, so behaviour is identical regardless of where the agent is launched from.
- The mini program now rejects connection targets that a real device can never use (plaintext `ws://` or a bare IP on a public host) as a configuration error instead of spinning in a reconnect loop. Local and intranet addresses stay allowed, since those are legitimate for DevTools and for the local smoke test.
- Replaced the smoke test's database copy with a SQLite online backup (`sqlite3.Connection.backup`). The desktop app holds `-wal`/`-shm` open around the clock, so `cp` failed on them and a main-file-only copy could miss commits that were still in the WAL. The copy is opened read-only and is transactionally consistent.
- Enabled `pipefail` in `scripts/e2e-smoke.sh`. Several steps ended in `命令 | tail`, which made `$?` report `tail`'s status and could mark a failed step as passing.

- Added a configurable pomodoro focus duration on the focus page, with quick presets (15/25/45/60/90 分钟) and a custom 1-120 分钟 input validated before starting a session.
- Added a 「记住番茄专注时长」 preference (front-end setting + persisted Rust settings key) so the chosen duration becomes the next session default; disabling it keeps the selection session-only.
- Added test coverage and a `test:focus-duration-preference` npm script asserting the validation rules and the full preference-saving wiring.
- Added a foreground rule mode setting with allowlist and blocklist semantics, reusing existing software, website and PotPlayer rules.
- Added hash-aware main navigation with `Alt+1` through `Alt+8` shortcuts and smoke coverage for keyboard routing.
- Added GitHub Actions CI for frontend type checking, frontend builds, Rust formatting, Clippy and Rust tests.
- Added repository hygiene files for editor defaults, dependency updates, toolchain hints, support, conduct, ownership and asset attribution.
- Added a public screenshot and demo asset policy under `docs/assets/`.
- Added focus-time bands behind the calendar timeline: real focus sessions for the selected day are drawn on the timeline background, tinted by subject colour, with a 「隐藏/显示专注底色」 toggle and a daily focus total in the timeline header.
- Added a `list_focus_sessions_in_range` Rust command that returns focus sessions overlapping a UTC time range, including the session that is still running.

- Clarified the calendar ringtone setting as 「日历铃声」 in Settings, with the existing schedule-reminder switch also exposed under the sound tab for easier discovery.
- Unified the subject palette behind a single source of truth: `--subject-*` variables in `src/styles.css`, `SUBJECT_PALETTE` in `src/palette.ts` and `DEFAULT_SUBJECTS` in `src-tauri/src/storage/db.rs`. Calendar category blocks, theme variants (dawn/sakura/forest) and focus bands now all derive from the same values, so a subject no longer renders in three different colours.
- Re-coloured the default subjects to remove semantic clashes: 政治 `#e5484d`, 英语 `#0ea5e9`, 数学 `#b45309`, 专业课 `#a855f7`. 数学 deliberately leaves the green family (the old `#16a34a` sat only ΔE 15.8 from the break green `#34c759`, which made focus blocks look like breaks).
- Changed the no-subject focus-band fallback from mint green `#4fd0a1` to the neutral slate `#94a3b8`.
- Retinted the focus-page progress accent from amber→coral to amber→yellow so it no longer collides with the 政治 red (ΔE was 18).
- Reworked the calendar pause band: a paused session's focus band still freezes at the pause moment, but the paused interval afterwards is now rendered with **no colour at all** (matching how break periods look on the timeline), instead of a separate slate-grey band. Removed the dedicated pause band, its hatching/pause-mark and the `--focus-band-pause` token, so 「专注 / 休息」 no longer share a hue on the timeline.
- Made the live study-mode state (`get_study_mode_state`) the authority for where a running focus band ends on the calendar timeline. Previously the band only honoured the `paused_at` carried on the session row, so any gap in that lookup let the band keep growing into the pause. Now a band freezes at `paused_at` when the current mode is paused and at `phase_started_at` during 休息 / 等待休息, and it no longer gets the 4-minute minimum height once frozen, so no colour spills into a non-focus interval. The state refreshes together with the per-minute focus-session refresh.

- Allowed foreground rules to be toggled while a normal study session is running, while keeping them locked on in strict mode.
- Added a setting to show or hide the foreground-rule toggle during an active study session.
- Changed Feishu task conflict resolution to use local and remote content fingerprints before timestamp arbitration, so local edits are not overwritten just because Feishu reports a newer remote timestamp.
- Standardized local check scripts so contributors can use cross-shell npm commands instead of Windows-only `npm.cmd` inside package scripts.
- Clarified that desktop releases are the default public path and Android release syncing is opt-in for maintainers.
- Raised the count-up (正计时) custom break limit from 60 分钟 to 720 分钟 (12 小时), across the focus page default, the manual-break dialog, the floating widget and the Rust validation range, so long rests such as overnight breaks no longer get clamped.
- Gave the count-up default break its own preset list (5/10/15/20/30/60/120/240/360/720 分钟) instead of reusing the pomodoro short-break presets, and made the settings stepper move 15 分钟 per click while direct input stays exact.

### Security

- Strengthened security reporting guidance to avoid publishing secrets, databases, sync backups or exploit details in public issues.

## v1.8.1 - 2026-06-07

### Added

- Added public-repository polish for the Windows/Tauri desktop app, including professional README metadata and clearer maintenance boundaries.
- Added and aligned release metadata for the current desktop line across `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.

### Changed

- Refined the release flow so Android project synchronization only runs when `--include-android` or `INCLUDE_ANDROID_RELEASE=1` is explicitly provided.
- Updated public documentation to present the project as a Windows local-first study focus app rather than a mixed desktop/mobile maintenance tree.

### Maintenance

- Kept dependency locks and release scripts aligned with the desktop-first GitHub publishing process.

## Historical notes

Earlier versions built the foundation of the product:

- Tauri 2 + React + TypeScript + Rust desktop shell.
- Local SQLite storage for study sessions, settings, review data and schedules.
- Study mode with focus/break cycles, long breaks, subject binding and status recovery.
- Windows foreground application and website allowlist checks.
- Checklist, today plan, schedule, daily review, weekly review and statistics pages.
- WebDAV and object-storage sync support with sync logs and backup restore flows.
- Optional Feishu task/calendar bridge, SMTP email reminders, PotPlayer detection and alarm reminders.
- Release automation for Windows installers and updater metadata.

Old auto-generated changelog entries with empty `No commits found` sections were collapsed here to keep the public history readable. Detailed archaeology remains available through Git tags and commit history.

## v1.8.2 - 2026-06-07

### Desktop

#### Added

- feat: polish app experience and release workflow (c527744)
- feat: add new components and hooks for improved user interaction and styling (4ff88b8)
- feat: update version to 1.7.4 and enhance release process with Android support (c3728aa)

#### Changed

- chore: bump version to 1.8.1 and update dependencies (5fe5f68)

## v1.8.3 - 2026-06-07

### Desktop

- No commits found.

## v1.8.4 - 2026-06-07

### Desktop

- No commits found.

## v1.9.0 - 2026-06-08

### Desktop

#### Changed

- Refactor focus study app flows and UI (31a171e)

## v1.9.1 - 2026-06-08

### Desktop

#### Changed

- Revert focus page UI to v1.8.4 and update core-flow smoke (00ba592)

## v1.9.2 - 2026-06-08

### Desktop

#### Changed

- Wake focus window for critical study reminders (20fa0f9)

## v1.9.3 - 2026-06-09

### Desktop

#### Added

- Add cleanup probes and system diagnostic tests (7e8cff9)

#### Changed

- Improve focus workflow and UI feedback across the app (8af17fd)

## v1.9.4 - 2026-06-09

### Desktop

#### Changed

- Unify light theme card surfaces (1988884)

## v1.10.0 - 2026-06-09

### Desktop

#### Added

- Add focus widget window for study mode (aaed2af)

## v1.11.0 - 2026-06-09

### Desktop

#### Fixed

- Fix task drag sorting theme colors (c488ec1)

## v1.11.1 - 2026-06-09

### Desktop

#### Added

- Add tray toggle for focus widget and refresh theme cards (714750b)

## v1.11.2 - 2026-06-09

### Desktop

#### Changed

- Show the focus widget for paused and idle study states (d5d845f)

## v1.11.3 - 2026-06-09

### Desktop

#### Changed

- Refine focus widget dock animations and glass motion (5e2615d)

## v1.11.4 - 2026-06-09

### Desktop

#### Changed

- Smooth focus widget collapse edges (f063ef7)

## v1.11.5 - 2026-06-09

### Desktop

#### Changed

- Suppress Windows title bar in focus widget (29a3f71)

## v1.12.0 - 2026-06-09

### Desktop

#### Changed

- Smooth focus widget retract countdown (854bde8)

## v1.12.1 - 2026-06-09

### Desktop

#### Changed

- Smooth focus widget expand animation (896a172)

## v1.12.2 - 2026-06-09

### Desktop

#### Changed

- Speed up focus widget collapse animation (e882edb)

## v1.12.3 - 2026-06-09

### Desktop

#### Changed

- Prevent focus widget collapse from blocking clicks (d0f13ab)

## v1.12.4 - 2026-06-10

### Desktop

#### Added

- Add completion reminders for finished study sessions (18549b6)

#### Changed

- Prevent focus widget from blocking main window input (e241184)

## v1.12.5 - 2026-06-10

### Desktop

#### Changed

- Restore global alarm watcher (cc9fde1)

## v1.12.6 - 2026-06-11

### Desktop

#### Changed

- Remove sample note and disable reminder sound timeout (a5fb836)

## v1.12.7 - 2026-06-11

### Desktop

#### Added

- feat: separate whitelist page content into tabs (204a12c)
- feat: add tab switcher component to whitelist page (e4793d7)
- feat: add tab state management for whitelist page (783082d)

#### Fixed

- fix: improve responsive tab styles - remove double border, add transition (d56a60a)
- fix: narrow transition properties for whitelist tabs (a41b08c)
- fix: correct indentation at line 688 in WhitelistPage.tsx (ded874d)

#### Changed

- chore: commit unrelated changes from other work (0645557)
- docs: add final report for whitelist tab layout optimization (cd31ea5)
- docs: update whitelist page optimization in FEATURES.md (5ce0e2a)
- style: add responsive styles for whitelist tabs (68383be)
- style: add whitelist tab switcher styles (16a0e6a)

## v1.12.8 - 2026-06-11

### Desktop

#### Added

- Add focus widget pause toggle (3efc48a)
- feat: add subject tabs inside whitelist rules view (e54cd5d)

#### Changed

- docs: update final report with subject tabs feature (d768d96)

## v1.12.9 - 2026-06-11

### Desktop

#### Changed

- Remove alarm sound auto-stop limit (8520fff)

## v1.12.10 - 2026-06-11

### Desktop

#### Added

- feat: complete UI redesign with Arc and Apple styles (31ed482)
- feat: implement Arc-style typography system (451f5d7)
- feat: implement Apple-style micro-interactions (ccb77b1)
- feat: implement Apple-style smooth animations (b742c18)
- feat: update --shadow variable to warm tones (51a5260)
- feat: implement warm shadow system with depth (e4d1373)
- feat: implement Arc-style warm color system (b26ec82)
- feat: implement Arc-style warm color system (d3c7eb7)
- feat: restructure main content with Arc-style layout (57e94f8)
- feat: restructure main content with Arc-style layout (ac74e11)
- feat: restructure sidebar with Arc-style warm design (a15ffc1)

## v1.12.11 - 2026-06-11

### Desktop

#### Added

- feat: 添加立即开始休息功能，更新相关UI和API调用 (ebd3bc5)
- feat: 移除侧边栏快速开始按钮，简化布局并清理未使用样式 (e889e00)

## v1.12.12 - 2026-06-11

### Desktop

#### Added

- feat: add update notification features and settings management (32bdf0c)

## v1.13.1 - 2026-06-11

### Desktop

#### Added

- feat: 移除立即开始休息功能及相关代码 (3f6eaf0)

## v1.13.2 - 2026-06-11

### Desktop

- No commits found.

## v1.13.3 - 2026-06-16

### Desktop

#### Changed

- 支持多片段网址白名单匹配 (b2ce781)

## v1.13.4 - 2026-06-17

### Desktop

#### Added

- Add study reminder timing and sound settings (dc1b0a7)

#### Changed

- Generalize Cargo lock version अपडेट for package name (9aa2a33)

## v1.13.5 - 2026-06-17

### Desktop

#### Added

- Add study reminder timing and sound settings (dc1b0a7)

#### Fixed

- Fix Feishu sync conflict handling (7a503e9)

#### Changed

- chore: release v1.13.4 (2d27536)
- Generalize Cargo lock version अपडेट for package name (9aa2a33)

## v1.13.6 - 2026-06-17

### Desktop

#### Changed

- Guard calendar sync against false remote deletions (74df496)

## v1.13.7 - 2026-06-18

### Desktop

#### Fixed

- 修复飞书日程和任务重复同步 (02cd9d4)

## v1.14.0 - 2026-06-18

### Desktop

#### Changed

- Refine Feishu sync conflicts and add foreground rule mode (0a3a4ee)

## v1.14.1 - 2026-06-18

### Desktop

#### Added

- Add CalDAV sync and rename schedule UI to calendar (abfef73)

## v1.14.3 - 2026-06-18

### Desktop

#### Changed

- Accept common CalDAV writable privileges in discovery (f32b061)

## v1.14.4 - 2026-06-18

### Desktop

#### Changed

- Preserve CalDAV UIDs during sync and add auto polling (b4a995e)

## v1.14.5 - 2026-06-18

### Desktop

#### Changed

- Separate allowlist and blocklist rule entries (562c4d9)

## v1.15.0 - 2026-06-20

### Desktop

#### Changed

- Improve dashboard analytics readability (d3ccb80)

## v1.15.2 - 2026-06-23

### Desktop

#### Added

- 新增学习趋势判断面板 (c80b0e0)

#### Changed

- Filter emergency exits from focus timeline (29da088)

## v1.15.3 - 2026-06-23

### Desktop

#### Changed

- 调整学习趋势为专注时间判断 (634bdde)

## v1.15.4 - 2026-07-14

### Desktop

#### Changed

- refactor(review): redesign retrospective dashboard (f7103d6)
- Use valid focus records in dashboard analytics (e388763)

## v1.15.5 - 2026-07-27

### Desktop

#### Added

- feat: skip breaks and add checklist tasks continuously (9fde76c)

#### Changed

- docs: record release verification (1fbfaaf)
- merge: skip breaks and continuous checklist entry (2ca9fb7)
- chore: release v1.15.4 (63def6f)
- refactor(review): redesign retrospective dashboard (f7103d6)
- Use valid focus records in dashboard analytics (e388763)

## v1.17.0 - 2026-07-28

### Desktop

- No commits found.

## v1.17.1 - 2026-07-28

### Desktop

- No commits found.

## v1.17.3 - 2026-07-28

### Desktop

- No commits found.

## v1.18.1 - 2026-07-29

### Desktop

#### Added

- feat: add responsive UI testing scripts with Playwright (dcca7f5)

#### Changed

- Cache native animation context for focus widget frames (b600db7)
- Refactor application structure and update UI behavior (e69f0e9)

## v1.18.4 - 2026-07-29

### Desktop

#### Fixed

- Merge branch '修复日历快速添加' (7c73c8b)
- fix: 修复快速添加功能，确保不包含已完成的今日事项 (8216fa5)
- fix: 更新学习模式通知信息，优化通知样式 (24fe032)
- Merge branch 'codex/修复悬浮窗卡顿' (cc7df2a)
- fix: 修复悬浮窗卡顿问题，优化窗口事件处理和动画准备逻辑 (10ca584)

#### Changed

- Merge commit '24fe032d66af62bebb9bcf026d812facaaa3a1c5' (a3abf4f)

## v1.18.5 - 2026-07-29

### Desktop

#### Fixed

- 修复卡顿 (62c3646)

#### Changed

- Merge commit '62c36468c52e13ac55fc09d304430fd88f3510f4' (51ab8e8)

## v1.18.6 - 2026-07-29

### Desktop

#### Changed

- 扩大了默认打开界面大小 (70f52a7)

## v1.18.7 - 2026-07-29

### Desktop

#### Changed

- 添加使用过程中能够控制前台规则 (05c0d2a)

## v1.18.8 - 2026-07-29

### Desktop

#### Changed

- 设置加控制 (cd4d621)

## v1.18.9 - 2026-07-30

### Desktop

#### Changed

- 增强设置面板样式，调整前台规则控制开关样式 (5c65aef)

## v1.19.0 - 2026-08-01

### Desktop

#### Fixed

- 修复 (97dba1f)
- 修复Command plugin:window|set_fullscreen not allowed by ACL (a735c29)

#### Changed

- chore: release v1.19.0 (67b8782)

## v1.19.1 - 2026-08-01

### Desktop

#### Fixed

- 修复 (97dba1f)
- 修复Command plugin:window|set_fullscreen not allowed by ACL (a735c29)

#### Changed

- 界面背景加了点呼吸感 (8b4cc5f)
- chore: release v1.19.0 (87fb65f)
- chore: release v1.19.0 (67b8782)

## v1.19.2 - 2026-08-01

### Desktop

#### Changed

- 优化呼吸 (e88b3bb)

## v1.19.3 - 2026-08-01

### Desktop

#### Changed

- 清单多日 (c108410)

## v1.19.4 - 2026-08-04

### Desktop

#### Fixed

- 修复日历无法删除周重复 (19ecb38)

## v1.19.5 - 2026-08-06

### Desktop

#### Fixed

- 修复日历重叠 (35d1037)

## v1.19.6 - 2026-08-06

### Desktop

#### Changed

- 日历模仿苹果 (7a7f2cf)

## v1.19.7 - 2026-08-07

### Desktop

#### Fixed

- 修复部分问题 (19f7b4e)

## v1.19.8 - 2026-08-07

### Desktop

#### Added

- Add idle UI hiding and ambient focus background animation (4fd0789)

#### Fixed

- Merge branch 'codex/沉浸式修复' (928ec15)

## v1.19.9 - 2026-08-07

### Desktop

#### Changed

- 单机实例 (b51010b)

## v1.20.0 - 2026-08-07

### Desktop

#### Fixed

- 修复部分bug (eab7570)

## v1.20.1 - 2026-08-13

### Desktop

#### Changed

- 铃声加控制按钮 (6b68d0c)

## v1.21.1 - 2026-09-03

### Desktop

#### Added

- feat: add configurable focus duration with persistence option (6608c07)

#### Changed

- recover working tree: 正计时专注模式 + 日历专注色带 (5f60bd4)

## v1.21.2 - 2026-09-03

### Desktop

#### Fixed

- fix(calendar): 暂停专注时冻结色带在暂停时刻，暂停区间保持空白 (f417c93)

## v1.21.4 - 2026-09-03

### Desktop

#### Fixed

- 修复 (d6d9efd)
- fix(calendar): 暂停区间改为中性灰独立色带，区别于专注色与休息绿 (859554d)

#### Changed

- refactor: 统一科目配色为唯一真源并消除跨语义色值冲突 (58ec950)

## v1.21.5 - 2026-09-03

### Desktop

#### Fixed

- fix(calendar): 暂停区间不再单独着色，与休息期一致显示为无颜色 (b365f2c)

## v1.21.6 - 2026-09-04

### Desktop

#### Fixed

- fix(calendar): 以学习模式实时状态为准冻结进行中的专注色带 (7e82209)

## v1.21.7 - 2026-09-06

### Desktop

#### Fixed

- fix (5472b00)

## v1.22.0 - 2026-09-25

### Desktop

#### Added

- Added an 「AI 智能日程规划」 integration: it reads checklist task attributes (title, priority, estimated minutes, due date, category), generates a non-overlapping timeline against your available windows and existing schedule, and presents the result as an editable draft that is only written to the calendar after you confirm.
- Added AI scheduler settings under 设置 → 集成 → AI 排期: provider preset (DeepSeek / OpenAI / custom gateway), base URL, model picker backed by `GET /models`, per-weekday available windows, peak windows, capacity parameters (default block length, minimum break, daily cap, `max_tokens`, temperature) and a connectivity test.
- Added `priority`, `estimated_minutes` and `ai_pinned` to checklist tasks, and `priority` / `estimated_minutes` to today-plan items. 「优先级」 and 「预计耗时」 are now editable in the shared task editor on both the checklist page and the today-plan drawer, instead of being display-only.
- Added `source_task_id`, `source_proposal_id` and `ai_locked` columns on schedule blocks plus an `ai_plan_proposals` draft table, so AI-generated blocks can be traced back to their source task and re-planned without disturbing manually arranged time.
- Added a first-enable disclosure dialog that lists exactly which fields leave the machine (task titles, optional notes, priority, estimated duration, due date, category names, existing schedule titles and time ranges, extra instructions) and which never do (focus records, statistics, review notes, allowlists, credentials).
- Added a local heuristic scheduling engine as the fallback path, so the feature stays usable when the model endpoint is unreachable.

#### Changed

- Bumped the cross-device sync payload to schema version 3 so the new task attributes travel between devices. Older payloads are still accepted and no longer clear locally-set values, because every new payload field is optional and `None` means "do not overwrite".

#### Security

- API keys are stored through Windows DPAPI. They are never persisted in plaintext and never returned to the frontend: the settings command reports only an `api_key_configured` boolean.
- AI endpoints are required to use HTTPS; plain `http` is accepted only for loopback addresses, so a self-hosted gateway on `127.0.0.1` still works without weakening the rule for remote hosts.
- The AI draft table is deliberately excluded from the cross-device sync export set, so in-progress drafts do not roam between devices.

#### Fixed

- Fixed a transitive build failure where `schemars 0.8.x` did not forward the `std` feature of `indexmap 1.9` into `tauri-build`'s build-dependency chain. Under Cargo resolver v2 features are not unified across `[dependencies]` and `[build-dependencies]`, so a plain `cargo build` failed with an `E0107` error on a clean checkout. `indexmap` is now declared in both sections.
- Fixed a `cargo fmt` violation in `src-tauri/src/commands/settings.rs` that made `npm run check:rust` fail.

#### Notes

- The AI planner is being delivered in stages. This release ships the data layer, the cross-device sync plumbing, the task-attribute editors and the settings screen with its connectivity gate. The draft preview drawer, drag-adjustment, apply/discard and one-click re-planning of affected windows land in follow-up builds. Until then, turning AI scheduling on only configures the provider and verifies that it is reachable.

## v1.22.1 - 2026-09-26

### Desktop

#### Fixed

- Fixed a critical defect that made AI scheduling unusable: the typed API key was never persisted. Saving the settings screen cleared `api_key` for redaction *before* the value was read, so the credential store always received an empty string and the app kept reporting 「请先在 设置 → 集成 → AI 排期 中填写 API Key」 no matter what you typed. The plaintext is now captured before normalisation, and three regression tests pin the ordering rule in place.
- Fixed 「测试连接」 failing even with a freshly typed key, because it read the credential store while the form was still unsaved. The button now saves first, then tests; a blank key still means "keep the existing one", so re-saving stays safe.
- Corrected the AI-scheduler error copy from 「设置 → AI 排期」 to the actual navigation path 「设置 → 集成 → AI 排期」, and retitled the 集成 tab to mention AI 排期 so the panel can be found.
- Fixed AI scheduling planning the wrong set of work: it pulled in **every unfinished checklist task across all five categories**, ignoring the 今日 / 计划 queue entirely. Scheduling now reads only the selected day's queue (unfinished entries; entries added to the queue by hand are included too), so what you put in the queue is what gets planned. This moved the scheduling unit from *checklist task* to *queue entry* through the whole pipeline, and a written block now links back to the queue entry it came from instead of fabricating an extra 今日计划 row.

#### Added

- Added the real AI scheduling path: 「生成草案」 now calls the configured model (DeepSeek / OpenAI / custom gateway) with structured-output negotiation, automatic retry with backoff (server `Retry-After` is honoured), and a schema-repair step that closes truncated or fenced JSON before giving up. Candidate items only carry ids and times — titles, categories, priorities and durations are always back-filled locally from the queue, so the model cannot invent entries or rewrite their names.
- Added an explicit 「改用本地排期」 escape hatch in the error card: AI failures now surface as errors instead of silently producing a local draft. Only after you press the button does the next generation fall back to the local heuristic, and the result is clearly labelled 「本地兜底排期」 instead of pretending to be an AI plan.
- Added the draft preview drawer: generate a plan, inspect it, then either write it to the calendar or discard it. Entry buttons sit on the checklist page header and the calendar page actions, and only appear once AI scheduling is enabled and the data-disclosure notice has been acknowledged.
- Added a read-only timeline view of a draft, grouped by day, showing each block's time range, category, duration and priority — plus any per-item warning attached to it.
- Added an explicit 「没排上的任务」 list to each draft, naming the tasks that could not be scheduled and why (no window long enough before the due date, daily capacity reached, or blocked by existing schedule), instead of reporting only a count.
- Added a local heuristic scheduler that produces a complete draft without contacting the model at all, so the drawer is fully usable before the AI path lands.
- Added per-item reasons after a write: when some entries are skipped, the drawer now lists exactly which ones and why (task deleted, no longer inside the current available windows, or conflicting with a named existing block).

#### Changed

- Moved the draft drawer from per-page state to an app-level singleton: switching to another page no longer closes the drawer or throws away an unconfirmed draft. The timeline stays open across navigation, and the checklist / calendar pages just refresh when a draft is written.
- The prompt engine badge now tells the truth about where a draft came from: 「AI 排期 · <model>」 for model output, 「本地兜底排期（AI 不可用时的降级结果）」 for explicit fallbacks, and 「本地排期（未使用 AI）」 only for legacy drafts created before the AI path existed.
- Writing a draft re-reads the *current* availability settings from the database rather than the copy taken when the draft was generated, so narrowing your windows between preview and confirmation correctly drops entries that no longer fit.
- Drift detection now also notices a changed estimated duration and a due date that has already passed, not just a changed priority.

#### Notes

- This build supersedes `v1.22.0`, which was packaged but never published and still contained the API-key defect.
- The AI planner is still being delivered in stages. Real model scheduling, draft preview, apply and discard now work end to end. Drag-adjustment of a draft and one-click re-planning of affected windows after a task changes land in follow-up builds.

## v1.3.1 - 2026-09-26

### Desktop
#### Added
- feat: miniapp-sync 小程序实时同步通道（Node 中继 + Python PC agent + 小程序） (ab58a82)
- feat: AI 智能日程规划（真模型排期 + 预览写入闭环），发布 1.22.1 (6b97daf)

#### Changed
- 11 (6000d89)
- 完成自动ai日程 (8c4130f)
- merge: AI 智能日程规划 1.22.1 并入 master (6e584af)

## v1.3.2 - 2026-09-26

### Desktop
#### Fixed
- 修复bug (38b30c8)

## v1.4.0 - 2026-09-30

### Desktop
#### Changed
- 加入白噪音混合功能 (5ca2a13)

## v1.4.1 - 2026-09-30

### Desktop
#### Changed
- 加入微休息的功能 (4b3a2f3)
- chore: release v1.4.0 (0418018)
- 加入白噪音混合功能 (5ca2a13)

## v1.5.0 - 2026-09-30

### Desktop
#### Fixed
- 修复ai排期 (c66cea2)

#### Changed
- chore: release v1.5.0 (cfe0b71)
- 完成？ (c9c774f)

## v1.5.1 - 2026-09-30

### Desktop
#### Fixed
- 修复ai排期 (c66cea2)

#### Changed
- 悬浮窗能够嵌入mydockfinder (0e28e23)
- chore: release v1.5.0 (94fb5ce)
- chore: release v1.5.0 (cfe0b71)
- 完成？ (c9c774f)



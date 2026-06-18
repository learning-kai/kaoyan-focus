# Changelog

All notable changes to `考研专注` will be documented here. The public desktop line follows semantic versioning where practical and focuses on user-visible changes, compatibility notes, security fixes and maintenance work.

## Unreleased

### Added

- Added a foreground rule mode setting with allowlist and blocklist semantics, reusing existing software, website and PotPlayer rules.
- Added hash-aware main navigation with `Alt+1` through `Alt+8` shortcuts and smoke coverage for keyboard routing.
- Added GitHub Actions CI for frontend type checking, frontend builds, Rust formatting, Clippy and Rust tests.
- Added repository hygiene files for editor defaults, dependency updates, toolchain hints, support, conduct, ownership and asset attribution.
- Added a public screenshot and demo asset policy under `docs/assets/`.

### Changed

- Changed Feishu task conflict resolution to use local and remote content fingerprints before timestamp arbitration, so local edits are not overwritten just because Feishu reports a newer remote timestamp.
- Standardized local check scripts so contributors can use cross-shell npm commands instead of Windows-only `npm.cmd` inside package scripts.
- Clarified that desktop releases are the default public path and Android release syncing is opt-in for maintainers.

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



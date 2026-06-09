# Changelog

All notable changes to `考研专注` will be documented here. The public desktop line follows semantic versioning where practical and focuses on user-visible changes, compatibility notes, security fixes and maintenance work.

## Unreleased

### Added

- Added hash-aware main navigation with `Alt+1` through `Alt+8` shortcuts and smoke coverage for keyboard routing.
- Added GitHub Actions CI for frontend type checking, frontend builds, Rust formatting, Clippy and Rust tests.
- Added repository hygiene files for editor defaults, dependency updates, toolchain hints, support, conduct, ownership and asset attribution.
- Added a public screenshot and demo asset policy under `docs/assets/`.

### Changed

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



# 考研专注

> A local-first Windows study focus app built with Tauri, React, TypeScript and Rust. It helps exam candidates run structured focus sessions, enforce app/site allowlists, review learning records, and optionally sync data across devices.

`考研专注` 是一个面向 Windows 桌面端的本地学习约束工具，目标是在备考期间把“开始学习、强制白名单、番茄节奏、任务计划、复盘统计、数据同步”串成一个可持续使用的学习工作台。

本项目第一版 GitHub 公开范围以 Windows / Tauri 桌面端为主。Android 相关内容不作为当前公开仓库主线维护范围。

## 核心特性

- 学习模式：支持总学习时长、番茄专注、短休、长休、长休间隔和科目绑定。
- 白名单约束：学习期间按软件、网站、PotPlayer 视频规则识别前台应用，减少干扰。
- 计划与复盘：提供清单、今日计划、课表、每日复盘、周复盘和学习统计。
- 本地优先：核心数据存储在本机 SQLite，默认不上传用户隐私数据。
- 云端同步：支持 WebDAV、对象存储 R2/S3 协议同步，并保留同步日志与备份恢复入口。
- 飞书与邮件：可选飞书任务/日历同步和邮件提醒。
- 桌面能力：系统托盘、后台 tick、前台检测、提醒音、自动更新检查。

## 截图与演示

发布 GitHub 前建议补充以下素材：

- 首屏主界面截图：`docs/assets/screenshot-focus.png`
- 设置页同步配置截图：`docs/assets/screenshot-settings-sync.png`
- 30 秒以内 GIF：展示开始学习模式、白名单拦截、统计回看。

不要提交包含真实个人数据、邮箱、飞书 App Secret、对象存储密钥或本机路径的截图。

## 快速开始

环境要求：

- Windows 10/11
- Node.js 20+
- Rust stable
- Microsoft C++ Build Tools / MSVC

安装依赖：

```powershell
npm.cmd install
```

开发预览：

```powershell
npm.cmd run dev
```

启动 Tauri 桌面端：

```powershell
npm.cmd run tauri dev
```

类型检查：

```powershell
npm.cmd run lint
```

完整检查（TypeScript、Rust 格式、Clippy、Rust 测试）：

```powershell
npm.cmd test
```

生产构建：

```powershell
npm.cmd run build
```

Rust 测试：

```powershell
cd src-tauri
cargo test
```

## 项目文档

建议按下面顺序阅读：

1. [项目文件结构说明.md](./项目文件结构说明.md)
2. [项目完整链路说明.md](./项目完整链路说明.md)
3. [项目开发规范（AI协作）.md](./项目开发规范（AI协作）.md)
4. [开发者AI开发与PR提交流程.md](./开发者AI开发与PR提交流程.md)
5. [FEATURES.md](./FEATURES.md)
6. [CHANGELOG.md](./CHANGELOG.md)

## 隐私与安全边界

- 应用默认本地运行，数据库位于 Tauri app data 目录。
- WebDAV、对象存储、飞书、SMTP 等凭据只用于用户主动配置的集成。
- 项目不实现驱动级拦截、不隐藏进程、不阻止任务管理器结束进程、不做恶意持久化。
- 请不要提交 `.env*`、私钥、真实数据库、同步备份、日志或构建产物。

更多说明见 [SECURITY.md](./SECURITY.md)。

## 发布与更新

桌面端发布脚本位于 `scripts/`。当前自动发布链路包含 Tauri Windows 构建、更新元数据生成、GitHub Release 上传等能力。公开发布资产、安装包签名和 `latest.json` 默认放在 [learning-kai/kaoyan-focus](https://github.com/learning-kai/kaoyan-focus) 的 GitHub Releases。

发布前先预检：

```powershell
npm.cmd run release:auto -- --version 1.7.4 --dry-run
```

第一版公开仓库以桌面端为主。Android 相关发布步骤是维护者内部流程，只有显式传入 `--include-android` 或设置 `INCLUDE_ANDROID_RELEASE=1` 时才会参与版本同步和 tag 校验。

## 路线图

- 补齐公开截图、演示 GIF 和英文快速介绍。
- 增加 GitHub Actions：前端类型检查、Rust 测试、Markdown 链接检查。
- 深化同步模块文档和备份恢复说明。
- 继续控制大文件增长，优先拆分 `sync_package/identity.rs` 和 `commands/sync/object_storage_protocol.rs`。
- 补充更细的贡献者任务标签，例如 `good first issue`、`docs`、`sync`、`windows`。

## 贡献

欢迎提交 issue、文档改进和功能 PR。开始前请阅读 [CONTRIBUTING.md](./CONTRIBUTING.md)。

如果你使用 AI 协助开发，请先让 AI 阅读：

- [项目文件结构说明.md](./项目文件结构说明.md)
- [项目完整链路说明.md](./项目完整链路说明.md)
- [项目开发规范（AI协作）.md](./项目开发规范（AI协作）.md)
- [开发者AI开发与PR提交流程.md](./开发者AI开发与PR提交流程.md)

## License

MIT License. See [LICENSE](./LICENSE).

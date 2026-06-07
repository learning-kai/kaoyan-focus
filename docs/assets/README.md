# 文档与演示素材规范

`docs/assets/` 用于存放公开 README、Release notes 或项目文档中展示的截图、GIF 和架构图。

## 推荐命名

- `screenshot-focus.png`：专注页主界面。
- `screenshot-settings-sync.png`：设置页同步配置。
- `screenshot-schedule.png`：课表页面。
- `demo-focus-flow.gif`：30 秒以内的核心流程演示。
- `architecture-overview.png`：架构示意图。

## 脱敏要求

提交素材前必须确认：

- 不包含真实姓名、邮箱、手机号、学校、课程安排或学习记录。
- 不包含 WebDAV、对象存储、飞书、SMTP 的 endpoint、bucket、access key、secret、token 或授权码。
- 不包含本机绝对路径、数据库路径、同步备份路径或日志全文。
- 不包含真实 SQLite 数据库、同步包、发布签名文件或 Tauri updater 私钥。
- 如果截图来自真实应用，请先使用测试数据或临时空数据库。

## 授权要求

- 截图和 GIF 默认应由项目维护者自行生成。
- 第三方图标、插画、音频、字体或模板必须在 [NOTICE.md](../../NOTICE.md) 中记录来源和授权。
- 授权不明确的素材不要提交到公开仓库。

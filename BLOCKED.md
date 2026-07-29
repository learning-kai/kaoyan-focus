# BLOCKED

无

## 已解决
- 2026-07-29：最终 `npm.cmd run check:rust` 首轮因 `spawn_focus_widget_geometry_animation` 触发 `clippy::too_many_arguments` 失败；改为动画请求结构体后继续验收。
- 2026-07-29：真实快速反向在 `167×144` 移回后仍落到旧目标，且 Tauri IPC 卡住；定位为后台线程持帧锁调用 `SetWindowPos` 的主线程互锁，已改为主线程帧提交，待复验。
- 以上为历史定位记录，不是当前阻塞；本轮 Rust 门禁、悬浮窗动效检查与 100%/150% DPI 响应式矩阵均已复验通过。

# PROGRESS · UI polish 1.17.1（2026-07-28）

## 任务0
- typecheck / test-ui-regression-fix / test-schedule-completed-mark 全绿，基线 1.17.0。
- 目标：时间轴可完成 > 字不溢出 > 卡片 radius-card=12 > 倒计时放大 > 美观。
- 顺序：时间轴完成 → 卡片/溢出 → 倒计时 → 脚本+1.17.1 包。
- 风险：HIG !important 再误伤布局。

## 任务1 时间轴一键完成
- schedule-block-actions 在 source_today_item_id 非空时渲染 Check 钮。
- 调用 handleCompleteTodayItem + stopPropagation；is-completed 保留。
- 反向：去掉时间轴 handleCompleteTodayItem → 脚本红；还原绿。

## 任务2 卡片统一 + 防溢出
- styles .schedule-block → border-radius: var(--radius-card,12px) + overflow:hidden + min-width:0。
- apple-hig .schedule-page .schedule-block 同步 radius-card；strong/span/small ellipsis。
- schedule-block 并入主卡组。

## 任务3 倒计时放大
- .focus-clock-zone strong → clamp(88px,16vw,140px)
- .timer-orbit strong → clamp(64px,10vw,112px)
- 全屏档 +约10%（103/24vmin/264 等）
- 反向：改回旧 clamp → 脚本红；还原绿。

## 任务4 脚本 + 包
- 新建 scripts/test-ui-polish-1171.mjs
- release:prepare --version 1.17.1
- tauri build → 考研专注_1.17.1_x64-setup.exe（约 8.38MB）

## 验收
- npm.cmd run typecheck 绿
- node scripts/test-ui-polish-1171.mjs 绿
- node scripts/test-ui-regression-fix.mjs 绿
- node scripts/test-schedule-completed-mark.mjs 绿
- 安装包：src-tauri/target/release/bundle/nsis/考研专注_1.17.1_x64-setup.exe
- 未 git commit；无新依赖

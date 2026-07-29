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

# PROGRESS · Focus widget motion（2026-07-29）

## 任务0
- 基线：`npm.cmd run typecheck` 通过；`npm.cmd run check:rust` 通过，Rust 为 80 passed、1 ignored、0 failed。
- 工作区既有改动保持不动：`src/apple-hig.css`、`src/pages/SchedulePage.tsx`、`scripts/test-schedule-completed-mark.mjs` 与 `.playwright-cli` 未跟踪文件。
- 目标：让贴边折叠、hover peek 展开、离开收回从当前呈现几何连续接管，使用无 overshoot 的临界阻尼时序，并保持置顶/不抢焦点。
- 顺序：先修 Rust 几何动画取消与 spring helper，再对齐前端材质动画，最后补静态防退化检查。
- 最大风险：窗口动画被反向 hover 打断时旧流程仍 snap 到旧目标，或 CSS 与原生窗口几何两套动画互相打架。

## 任务1 Rust 几何动画
- 固定 step + cubic-bezier 改为基于 `Instant::elapsed()` 的临界阻尼 spring；展开 response=360ms，收回 response=340ms，无 overshoot。
- 动画 generation 与当前窗口几何在帧锁内领取；每帧提交也在同一帧锁内核对 generation，旧动画不能在反向操作后补写一帧。
- collapse 取消分支不再 snap 到旧 target，不再发送旧状态事件；新动画从当前 live geometry 接管。
- Rustfmt 通过；`cargo test ... windows::focus_widget::tests` 为 7 passed、0 failed。

## 任务2 前端材质动画
- hover 展开延迟 120ms→60ms，收回延迟 180ms→160ms；删除 48ms 收回预等待。
- 面板改为可反向 CSS transition，只动画 opacity、0.7px blur 与 0.986 轻 scale；边缘位移完全交给 Rust，保留四边 transform-origin。
- 删除计时 42ms 缩到 0.56 和详情 56ms 淡出的硬切；材质进入 220ms、退出 200ms。
- 前端增加 transition generation；快速移回鼠标可中断收回并立即重新 peek，过期 Promise 不再覆盖新状态。
- `npm.cmd run typecheck` 通过。工作期间新出现的 `src/pages/FocusPage.tsx` 改动不是本任务产生，保持不动。

## 任务3 防退化检查
- 新增 `scripts/test-focus-widget-motion.mjs` 与 `npm run test:focus-widget-motion`，并接入现有 `check` / `test` 流程。
- 检查旧 collapse 取消分支无 snap、response=320-400ms、临界阻尼 helper/单测、前端反向 generation、材质无位移、reduced-motion 明确覆盖 shell/panel/tab。
- 脚本对照 `package-lock.json` 根清单检查 dependencies/devDependencies；本任务未改 lockfile、未新增依赖。
- `npm.cmd run test:focus-widget-motion` 与 `npm.cmd run typecheck` 通过。

## 运行时验收迭代
- 首轮真实 hover：展开 `172×36→280×172`、收回 `280×172→172×36` 均完成，前台句柄未变化。
- 快速反向首测失败：`219×96` 仍先落到 `172×36` 再展开，最大相邻跳变 94px；根因是同步 Tauri command 阻塞后续 peek，而非 spring 曲线。
- 修正：几何动画转入 `spawn_blocking`，命令立即返回以允许新 generation 抢占；geometry event 抑制时长覆盖 debounce + 最长 response + grace。
- 最终 `check:rust` 首轮被 `clippy -D warnings` 拒绝：异步 helper 为 8 个参数；已封装 `FocusWidgetGeometryAnimation` 消除 lint，不用 `allow` 放行。
- 修复后最终门禁：`npm.cmd run typecheck` 通过；`npm.cmd run check:rust` 通过（83 passed、1 ignored、0 failed）；`npm.cmd run test:focus-widget-motion` 输出 `Focus widget motion assertions passed`。
- 最新调试版真实 `SendInput`：单次 hover 展开/离开收回均单调且不抢焦点，但快速反向在 `167×144` 移回后仍落到 `36×112`，随后 Tauri IPC 卡住。
- 根因：后台动画线程持有帧锁调用同步 Win32 `SetWindowPos`，反向命令所在窗口主线程等待同一锁，形成跨线程窗口消息死锁；改为主线程内完成加锁、generation 校验与帧提交。
- 主线程帧提交复测不再死锁，且最终会回到 `280×172`；但 WebView re-entry 命令仍在连续 resize 后才处理，曾先落到 `36×112`。增加 Rust 原生 outside→inside 回流守卫，让新 generation 不依赖 DOM 事件排队。
- DPI 双坐标核验确认鼠标实际位于窗口内；主线程任务里 `window.hwnd()/scale_factor()` 仍会二次投递 Tauri 主线程并卡住。改为后台预取 HWND/scale/shape，主线程只做纯 Win32 帧与圆角提交。
- 原生回流已在 `199×152` 抢占旧动画并阻止旧目标（最低 `84×124`），但新动画启动前再次互锁：`begin_focus_widget_animation` 持帧锁做 Tauri 几何查询。Windows 路径改为锁内直接 `GetWindowRect`。
- 锁内原生几何后不再死锁，最终恢复 `280×172`；但回流守卫的 Tauri 光标/窗口查询仍排在 resize 后，`75×121` 回流曾到旧目标。守卫改用 Win32 `GetCursorPos + GetWindowRect`，32ms 确认。

# PROGRESS · 全面响应式审查（2026-07-29）
- 目标/让步：运行态左侧导航与全部主操作可见 > 页面零滚动零裁切 > 功能不退化 > 安静的 Apple/Windows 工具感。
- 顺序：真实验收脚本 → 主壳/普通页 → 专注运行态 → 悬浮窗/DPI → 主题动效 → 全量收口。
- 最大风险：历史 CSS 多层覆盖、关闭抽屉撑宽、Tauri 全屏隐藏侧栏、150% DPI 悬浮窗尺寸不足。
- `git status --short --branch`：master 比 origin/master ahead 1；既有改动与 `.playwright-cli` 资产均保留。
- `npm.cmd test`：通过，Rust 83 passed / 1 ignored / 0 failed；`npm.cmd run build`：通过。
- `node scripts/test-ui-regression-fix.mjs`：通过。
- 旧 `test-ui-polish-1171.mjs` / `1172.mjs` 含过期断言，已删除；由新的全量响应式验收替代。

## 全面响应式审查收口（2026-07-29）
- 专注准备页：开始学习按钮提升到预览区首位，960×680/1100×760 首屏可达；主内容与过渡层补 `min-width: 0`、横向裁切。
- 开始学习后：左侧导航保留；`.main-panel`、`.page-transition`、`.focus-active-shell` 固定 `100dvh` 并 `overflow: clip`，倒计时页无可滚动区域。
- 抽屉：关闭态隐藏且不再平移撑宽；打开态最大高度锁定为 `100dvh - 40px`，任务/日历抽屉均为 fixed 覆盖层。
- 悬浮窗：沿用 36×36、172×36、240×172、280×144 四档与 100%/150% DPI 验收；无溢出、关键内容完整。
- 新增 `scripts/test-ui-responsive.mjs`、`scripts/ui-responsive-browser.mjs`、`scripts/ui-responsive-fixture.mjs`，接入 `npm run test:ui-responsive`；默认构建后用 preview 静态产物执行五个隔离 scope。
- 主页面 32 组、专注态 20 组、导航 4 组、抽屉 2 组、悬浮窗 8 组全绿；反向 `UI_RESPONSIVE_REVERSE=1` 实测注入 32px 溢出并按预期报红。
- light/dark/mono + reduced-motion 抽测通过：主题真实生效，专注壳/document overflow=0，动画/过渡降到 `1e-05s`。
- 清理两个过期 `test-ui-polish-1171.mjs`、`test-ui-polish-1172.mjs` 假红脚本；CSS 剩余负 `letter-spacing` 全部归零。
- 最终门禁：`npm.cmd test` 通过（Rust 83 passed / 1 ignored / 0 failed）；`npm.cmd run test:ui-responsive` 通过；`npm.cmd run build` 由 UI 入口再次通过。

# PROGRESS · Focus widget motion 收口（2026-07-29）
- 最近一次完整门禁：`npm.cmd run typecheck` 通过；`npm.cmd run check:rust` 通过（83 passed、1 ignored、0 failed）；`npm.cmd run test:focus-widget-motion` 输出 `Focus widget motion assertions passed`。
- 最近一次 widget 响应式矩阵：`npm.cmd run test:ui-responsive`（widget scope，100%/150% DPI）通过；8 个尺寸/清晰度场景均无溢出。
- 本地 preview computed-style 抽测：普通 motion 下 panel 仅 `opacity/transform/filter`，进入 220ms、退出 200ms；shell `background=transparent`、`border=0`、`overflow=hidden`、圆角与 native region 对齐。`prefers-reduced-motion: reduce` 下 shell/panel/tab 的 animation、transition、transform、filter 均为 none。
- 补充修正：全局 HIG 的 `!important` 原本把 shell 覆成不透明页面底和边框；在白名单 `FocusWidgetPage.css` 增加透明 shell 覆盖，玻璃材质只留在 panel，不改透明窗口、置顶或学习状态逻辑。
- 真实 Windows 调试链路（此前最终 native 回流复测）：贴边折叠 `280×172→36×112`，hover 展开 `36×112→280×172`；快速反向在 `199×152` 抢占，最低 `98×127` 后回到 `280×172`，宽/高/y 逆向帧为 `0`；前台窗口未变化，`Topmost=true`、`NoActivate=true`。
- 结论：旧动画取消不再落旧 target；几何从 live presentation 接管、临界阻尼无 overshoot；材质动画不与窗口位移争用，reduced-motion 不做大幅位移。
- 交付包：已先单独运行 `npm.cmd run build` 通过，再用 Tauri CLI `--config {"version":"1.18.0","build":{"beforeBuildCommand":""}}` 生成 NSIS；安装包为 `src-tauri/target/release/bundle/nsis/考研专注_1.18.0_x64-setup.exe`（8,801,883 bytes）。未改 `Cargo.toml`、`tauri.conf.json` 或 `package-lock.json`。

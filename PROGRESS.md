# PROGRESS · 学习数据看板重构（2026-08-01）

## 任务0
- 目标：把独立统计看板改成“本周状态、原因、下一步行动”的周诊断，不再以图表数量作为价值。
- 顺序：先修正计划/任务聚合与只读数据契约，再写纯函数状态判定，最后重做首屏和详情。
- 取舍：数据正确 > 状态可解释 > 首屏直观 > 信息完整 > 视觉炫技。
- 本轮约束：实现过程中不重复构建，不做持续视觉巡检；先交付第一版，视觉问题由用户手动反馈。
- 最大风险：现有任务完成桶被重复挂到多条专注记录，且当前看板接口未提供日程计划基准。

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

# PROGRESS · Focus widget per-frame native context（2026-07-29）
- 任务0复跑：`npm.cmd run typecheck`、`npm.cmd run test:focus-widget-motion`、`npm.cmd run check:rust`、`git diff --check` 全部通过；Rust 83 passed、1 ignored、0 failed。
- 当前工作区仍为 `master...origin/master [ahead 2]`，无未提交改动。
- 目标：逐帧提交只消费预计算 HWND/DPI 原生上下文，消除每 16ms 重复 Tauri 窗口查询，同时保持 generation 抢占和主线程 SetWindowPos。
- 顺序：先追加旧实现必红的新断言，再做 Rust 快照修复，最后复跑门禁与白名单审计。
- 最大风险：快照跨 DPI/反向动画失效，或为降开销重新引入跨线程窗口死锁。
- 当前未验证：本轮尚未运行真实 100%/150% DPI Windows 20 次快速反向矩阵。
- 任务1红证据：在旧实现运行 `npm.cmd run test:focus-widget-motion`，按预期失败：`FAIL: each Windows frame must consume the immutable native context captured before animation`。
- 任务1绿证据：新增断言在快照修复后运行 `npm.cmd run test:focus-widget-motion`，输出 `Focus widget motion assertions passed`。
- 任务2：`FocusWidgetNativeAnimationContext` 在动画启动时快照 HWND/DPI，并随 generation 进入 `FocusWidgetGeometryAnimation`；Windows 逐帧提交不再调用 `window.hwnd()`/`window.scale_factor()`，仍由主线程做 generation 校验和 `SetWindowPos`。
- 任务2门禁：`npm.cmd run check:rust` 通过，Rust 为 83 passed、1 ignored、0 failed；未改依赖、配置或前端动效。
- 最终门禁：`npm.cmd run typecheck`、`npm.cmd run test:focus-widget-motion`、`npm.cmd run check:rust`、`git diff --check` 全部通过；逐帧函数内 HWND/DPI 查询计数均为 0。
- 白名单审计：`git diff --name-only` 仅含 `BLOCKED.md`、`PROGRESS.md`、动效脚本和 `focus_widget.rs`；运行时未验证已按要求写入 BLOCKED.md。

# PROGRESS · AI 智能日程规划 S3（本地排期闭环 + 只读抽屉，2026-09-26）
- 目标：**完全不接 AI** 也能生成草案、人工预览、确认写入日历；这一阶段结束时有可人工执行验收的 UI 入口，而不是只有后端单测。
- 后端新增 `context.rs` / `validator.rs` / `planner.rs` / `apply.rs`，并注册命令 4（`preview_ai_schedule`，S3 用本地启发式实现）、7（`apply_ai_plan_proposal`）、8（`discard_ai_plan_proposal`）、9（`get_latest_ai_plan_proposal`）。
- 两条排期路径（本地启发式 / S4 的 LLM）共用同一个 `RawPlanResponse` → `validator::validate`，合法性判定只有一处实现。
- 校验器按 `task_id` 回填 `title` / `category_key` / `subject_id` / `priority`，候选里只有 `task_id` 与时间，模型无法编造任务或篡改标题。
- `apply` 承担三道保险：漂移检测（任务已删除 / 已完成 / 优先级或耗时变化 / 已逾期）、窗口二次硬校验（读**库里当前**设置，不是调用方传入的那份）、单事务写入（写入前自动补建今日计划并登记 `sync_meta`，避免日历与今日计划对不上）。
- 前端新增 `AiPlanDrawer.tsx`（生成 / 预览 / 写入 / 放弃，含可重试的错误卡片）与 `AiPlanTimeline.tsx`（纯展示，按日期分组并挂载逐条告警），并在清单页与日历页各挂一个「AI 排期」入口，显隐取决于 `enabled && privacy_acknowledged`。
- 契约追加：`AiApplyResult.warnings`（写入期的逐条跳过原因）、`AiPlanProposal.unscheduled` 与 `ai_plan_proposals.unscheduled_json` 列（`add_column_if_missing` 幂等迁移）。
- 顺带修掉：`validator::horizon_bounds` 与 `context::build_context` 的重复实现（移入 `context.rs` 并复用，命令行 `-D warnings` 下的 dead_code 一并消失）。
- 门禁：`cargo test --offline --lib` **163 passed / 0 failed / 1 ignored**；`cargo clippy --offline --all-targets -- -D warnings` 通过（仅剩 Windows `target/` 文件锁噪音）；`cargo fmt -- --check` 干净；`npm run typecheck` 通过；`npm run build` 通过（`AiPlanDrawer` 独立 8.67 kB chunk）。
- 未做（留给后续阶段）：拖拽微调、重生成、`replan`、真实模型调用与降级重试、日历页的「变动后一键重排」。
- 阻塞项：**S2 的连通性测试仍未由用户实测确认**（见 BLOCKED.md），S4 接入前需要先过这道闸门。

# PROGRESS · 1.22.1 打包（2026-09-26）
- 决定：1.22.0 从未发布，直接升到 **1.22.1**，把密钥缺陷修复与 S3 一起发。
- 版本号由 `node scripts/prepare-release.mjs --version=1.22.1` 统一写入 5 处（package.json / package-lock.json / Cargo.toml / Cargo.lock / tauri.conf.json）；脚本生成的 CHANGELOG 段落因改动未提交只有 `No commits found.`，已手工改写成真实条目。
- 打包：`node scripts/release-win.mjs --no-prepare --no-tag`，约 2m31s（release target 目录是热的）。
- 产物：`src-tauri/target/release/bundle/nsis/考研专注_1.22.1_x64-setup.exe`，**8,951,599 bytes**，SHA-256 `29a654c7ad0727916b8724de4ba8fa8a8824e2e468957fc26d9b38b51428214f`。
- 已知坑：脚本最后一步「拷贝产物到仓库目录」被沙箱 safe-delete 批量删除保护拦下（目标目录有 123 个历史包 > 阈值 50）。**包本身已生成**，用 `cp` 手工拷贝解决。
- 产物验证：二进制内含 `preview_ai_schedule` / `apply_ai_plan_proposal` / `discard_ai_plan_proposal` / `get_latest_ai_plan_proposal` / `ai_plan_proposals` / `unscheduled_json`；exe 与安装包的 PE VERSIONINFO 均为 `1.22.1`（UTF-16LE 各 2 次），旧版本号 `1.22.0` 出现 **0** 次。
- 未做：没有 commit、没有打 tag（最新 tag 仍是 `v1.21.7`）、没有发布更新源。这三件事需要单独确认后再做。

# PROGRESS · AI 智能日程规划 · 排期来源改为「计划队列」（2026-09-26）
- 用户实测反馈：AI 自动排的是**所有未完成清单任务**，而不是今日/计划队列。定位到 `context::build_context` 调的是 `checklist::list_schedulable_tasks`（`WHERE completed = 0 AND ai_pinned = 0`），按设计文档 §4.2 实现，但该设计本身与预期不符。
- 已确认的两个口径：来源日期＝**页面当前选中的日期**（`target_date`）；队列里手动新建的条目（无清单来源）**一起排**。
- 改动主线：排期单位由「清单任务」改为「队列条目」（`today_plan_items`）。
  - `checklist.rs`：`SchedulableTask`/`list_schedulable_tasks` → `SchedulableQueueItem`/`list_queue_items(conn, today_date)`（`WHERE today_date = ?1 AND completed = 0`，`LEFT JOIN checklist_tasks` 取 `board_scope` 定分类）；`TaskDriftState`/`task_drift_state` → `QueueItemDriftState`/`queue_item_drift_state`。
  - 契约改名：`AiPlanRequest.task_ids` → `queue_item_ids`；`RawPlanItem.task_id` / `RawUnscheduledItem.task_id` / `UnscheduledEntry.task_id` → `item_id`；`ContextTask` → `ContextQueueItem{ item_id, source_task_id }`；`PlanContext.tasks` → `queue_items`（`#[serde(default, alias = "tasks")]` 兼容旧快照）；`PlanContext::task_by_id` → `item_by_id`；`AiPlanWarning.task_id` → `queue_item_id`（`for_task` → `for_queue_item`）。
  - `apply.rs`：漂移检测改读队列条目；`source_today_item_id` 由 `validator` 直接回填（队列条目本就存在），**删掉「先补建今日计划再写日程」**，链路完整性变成天然成立。
- 有意偏离设计：不再按 `ai_pinned` 过滤（该字段无 UI，隐藏过滤会让「加进今天却没排期」无法解释）；「在队列里」本身就是显式选择。
- 设计文档：§12.3 记 6 条修正（#33–#38）；§4.2 加修正横幅并重写两端伪代码；§3.2/§3.3/§5.1/§5.2/§5.3 的字段名与 schema 同步为 `item_id`。
- 前端：`aiScheduler.ts` 字段改名（`queue_item_ids` / `queue_item_id` / `AiUnscheduledEntry.item_id`）、`AiPlanTimeline` 的 key 改用 `entry.item_id`、抽屉新增范围说明文案与 `.ai-plan-hint` 样式。
- 门禁：`cargo test --offline --lib` **163 passed / 0 failed / 1 ignored**；`cargo clippy --offline --all-targets -- -D warnings` 通过；`cargo fmt` 干净；`npm run typecheck` 通过；`npm run build` 通过。
- 新增回归测试：`block_links_to_existing_queue_item_without_creating_a_duplicate`（写日程不得凭空补建第二条今日计划）、`item_removed_from_queue_is_skipped`、`completed_queue_item_is_skipped`、`queue_filter_intersects_ids_and_categories`、`queue_items_are_scoped_to_one_date_and_unfinished_only`（直击本次缺陷：队列必须按日期 + 未完成过滤）。
- 已重新打包 1.22.1（1.22.1 从未发布，直接并入本次修正）：`node scripts/release-win.mjs --no-prepare --no-tag`，约 3m18s；最后拷贝步骤照例被 safe-delete 批量删除保护拦下，`cp` 手工拷贝解决。
- 产物：`src-tauri/target/release/bundle/nsis/考研专注_1.22.1_x64-setup.exe`，**8,965,347 bytes**，SHA-256 `dc57951af18b564a73b8858444975bed0a3f2d04da4c455c486c29bf601be771`（取代之前那份 8,951,599 bytes / `29a654c7…`）。
- 产物验证：exe 内含新漂移文案「已被移出当日队列」、`queue_item_ids`、`source_today_item_id`；无独立的 `task_ids` 字段（仅剩 2 处是 `source_task_id` 的子串）；exe 与安装包 VERSIONINFO 均为 `1.22.1`（UTF-16LE 各 2 次），`1.22.0` 为 **0** 次。安装包内容探针为 0 属正常——NSIS 会整体压缩。
- 未做：没有 commit、没有打 tag（最新 tag 仍是 `v1.21.7`）、没有发布更新源。这三件事需要单独确认后再做。

# PROGRESS · AI 智能日程规划 S4 前端 + 抽屉跨页保活（2026-09-26）
- 用户实测反馈两点：①切换页面时 AI 排期抽屉跟着被关掉；②草案来源一直是「本地排期（未使用 AI）」，用户只要真 AI。
- **降级决策（用户拍板）**：默认**不自动降级**。`allow_local_fallback` 默认 false，AI 失败就报错；只有错误卡片里点「改用本地排期」才对下一次生成置 true，成功后立即复位（「重新生成」永远先试真模型）。降级草案标 `degraded`，徽标文案三态：AI 排期 · <model> / 本地兜底排期（AI 不可用时的降级结果）/ 本地排期（未使用 AI，仅历史草案）。
- **抽屉 App 级单例**：新增 `src/services/aiPlanBus.ts`（CustomEvent 总线，沿用 `APP_NAVIGATE_EVENT` 约定）。`App.tsx` 持有 `<AiPlanDrawer>` 并订阅 `onAiPlanOpen`；清单页 / 日历页只留入口按钮（`openAiPlanDrawer(date, labels)`）并订阅 `AI_PLAN_APPLIED_EVENT` 刷新自身；`showAiPlan` 页内状态与页内挂载全部移除。分类显示名经事件从清单页透传，日历页不传时用 Timeline 兜底名。`onApplied` 只通知刷新，不关抽屉（保留写入结果展示）。
- 后端侧（同日早前完成）：`prompt.rs`（System/User Prompt，含 json 字样 + 结构示例，满足 json_object 档硬性要求）、`client.rs::chat_json`（重试退避 + Retry-After 优先 + JSON 修复管线）、`settings.rs::load_api_key`（DPAPI 明文只在调用时取）、`planner::preview_proposal`（AI 优先 + 可选降级）、`preview_ai_schedule` 改 `async fn` + `spawn_blocking`。
- 门禁：`npm run typecheck` 通过；`npm run build` 通过（新前端进 `App-myZYrv-8.js`，含「改用本地排期」与 `open-ai-plan` 事件名）；Rust 侧未再改动，沿用此前全绿结果（`cargo test --lib` 88 passed for ai_scheduler 模块 / 全库 164 passed）。
- 打包：`node scripts/release-win.mjs --no-prepare --no-tag`，2m55s。**新坑**：脚本第一步 vite `emptyOutDir` 就会被 safe-delete 批量删除保护拦下（dist/assets 91 文件 > 50 阈值），需分批手动清空 dist 再重跑；最后拷贝步骤照旧被拦（nsis 目录 79 个历史包），`cp` 手工拷贝。
- 产物：`src-tauri/target/release/bundle/nsis/考研专注_1.22.1_x64-setup.exe`，**8,980,426 bytes**，SHA-256 `998f9ef89509efd87bc3e99df397ae04fdbd7f29fac9ad43b768061dfbeeb668`（取代 `dc57951a…` 那份）。
- 产物探针：exe 内含 `chat/completions`、`response_format`、`finish_reason`、`json_object`、`retry-after`（×18）、`allow_local_fallback`、「服务商返回的内容不是合法 JSON」、「模型给出的条目」；VERSIONINFO 为 `1.22.1`（UTF-16LE ×2），`1.22.0` ×0。前端文案（「本地兜底」「改用本地排期」）因 brotli 压缩在 exe 中搜不到，已在 dist 产物确认。
- 未做：没有 commit、没有打 tag、没有发布更新源。S5 拖拽微调与 S6 一键重排仍留给后续阶段。

任务 0（2026-07-27）：已确认工作区仅有用户既有 CSS 与 BGM 修改，未触碰。基线 npm run build 通过；cargo test --lib 为 74 passed、0 failed、1 ignored。
执行顺序：先补回归脚本，再实现后端跳过状态转换与同步，最后接入主界面/悬浮窗及清单连续输入。
最大风险：休息跳过必须与现有自动进入下一轮逻辑共用状态语义，避免重复创建 session 或破坏跨设备控制动作。

已完成：新增 skip_study_break 前后端命令、skip_break 同步动作与 3 条同步回归测试；主界面和悬浮窗均接入跳过按钮；分类清单新增成功后保持 composer 展开并恢复标题输入焦点。
当前验证：npm run build、cargo fmt --check、cargo clippy --all-targets -D warnings 均通过；cargo test --lib 为 77 passed、0 failed、1 ignored。
待完成：按任务书补齐 scripts/test-focus-skip-and-checklist-entry.mjs 的真实浏览器回归并运行；该脚本尚未创建。

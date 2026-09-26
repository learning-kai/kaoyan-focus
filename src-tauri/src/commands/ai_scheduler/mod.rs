//! AI 智能日程规划。
//!
//! 对外路径保持 `commands::ai_scheduler::xxx`，与既有 `commands::feishu` 一致。
//! 设计文档见 `docs/AI智能日程规划实现方案.md`。
//!
//! 模块分工（与方案 §8.1 对应）：
//! - `models`：全部共享数据结构与常量
//! - `settings`：命令 1–3（读取 / 保存 / 连通性测试），密钥走 DPAPI
//! - `context`：把队列条目、日程、时段收成一份 `PlanContext` 快照
//! - `prompt`：系统 / 用户提示词（S4）
//! - `validator`：硬校验 + 按 `item_id` 回填字段，拒绝模型幻觉
//! - `planner`：模型排期编排（本地启发式仅作显式兜底）+ 草案落库
//! - `client`：Chat Completions 调用与档位探测
//! - `apply`：命令 4（预览）、7（写入日历）、8（丢弃草案）、9（恢复草案）
//!
//! 后续阶段新增 `replan.rs`（S6）。

pub mod apply;
pub mod client;
pub mod context;
pub mod models;
pub mod planner;
pub mod prompt;
pub mod settings;
pub mod validator;

use tauri::{AppHandle, Manager};

/// 应用数据库路径。各子模块共用，避免每个文件各写一份。
pub(super) fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("kaoyan-focus.sqlite3"))
}

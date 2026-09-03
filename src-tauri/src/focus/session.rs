use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FocusSession {
    pub id: i64,
    pub mode: String,
    pub subject_id: Option<i64>,
    pub planned_seconds: i64,
    pub actual_seconds: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub end_reason: Option<String>,
    pub interruption_count: i64,
    pub emergency_exit_count: i64,
    pub created_at: String,
    pub updated_at: String,
    /// 进行中且被暂停时，记录暂停发生的时刻（取 study_modes.paused_at）。
    /// 前端据此把专注色带冻结在暂停那一刻，而不是一直延伸到当前时钟。
    pub paused_at: Option<String>,
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FocusSession {
    pub id: i64,
    pub mode: String,
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
}

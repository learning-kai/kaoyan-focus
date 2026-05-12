use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Subject {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WhitelistApp {
    pub id: i64,
    pub name: String,
    pub process_name: String,
    pub path: Option<String>,
    pub match_type: String,
    pub note: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

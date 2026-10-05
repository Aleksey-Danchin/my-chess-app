//! Chess engine manager boundary.
//!
//! MVP: stubs only. Future work will download/install UCI binaries and speak UCI.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
}

pub struct EngineManager;

impl EngineManager {
    pub fn list_installed() -> Result<Vec<EngineInfo>, String> {
        Err("not_implemented".into())
    }

    pub fn install(_engine_id: String) -> Result<(), String> {
        Err("not_implemented".into())
    }
}

mod catalog;
mod install;
mod profile;
mod threads;
mod types;
mod uci;

pub use catalog::{list_catalog, CatalogEngine};
pub use install::{install_engine, list_installed, remove_engine};
pub use profile::{
    create_profile, delete_profile, list_profiles, update_profile, CreateProfileInput,
    UpdateProfileInput,
};
pub use threads::{clamp_threads, max_allowed_threads};
pub use types::{AnalysisLine, AnalysisProfile, AnalysisSettings, GoMode};
pub use uci::{get_best_move, AnalysisManager};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: String,
    pub installed: bool,
}

mod catalog;
mod install;

pub use catalog::{list_catalog, CatalogEngine};
pub use install::{install_engine, list_installed, remove_engine};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: String,
    pub installed: bool,
}

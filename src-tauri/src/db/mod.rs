mod catalog;
mod games;
mod install;

pub use catalog::{fetch_catalog, CatalogDatabase};
pub use games::{list_games, GamesPage};
pub use install::{install_database, list_installed, remove_database};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub id: String,
    pub title: String,
    pub path: String,
    pub game_count: u64,
    pub storage_size: u64,
    pub installed: bool,
}

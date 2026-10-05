mod catalog;
mod encoding;
mod games;
mod install;
mod pgn_import;
mod prune;
mod result_col;
mod search;
mod search_index;
mod user_download;

pub use catalog::{fetch_catalog, CatalogDatabase};
pub use encoding::{chess_from_fen, fen_key, move_to_uci};
pub use games::{list_games, GamesPage};
pub use install::{install_database, list_installed, remove_database};
pub use search::{
    play_uci_on_fen, search_position, search_position_with_progress, PositionSearchResult,
};
pub use search_index::{build_index, index_status};
pub use user_download::download_user_database;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub id: String,
    pub title: String,
    pub path: String,
    pub game_count: u64,
    pub storage_size: u64,
    pub installed: bool,
    pub indexed: bool,
}

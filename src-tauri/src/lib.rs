mod db;
mod engine;
mod fs;
mod game;

use std::sync::Mutex;

use fs::DownloadCanceller;
use game::{Game, PositionSnapshot, SharedGame};
use tauri::{AppHandle, State};

#[tauri::command]
fn get_position(game: State<'_, SharedGame>) -> Result<PositionSnapshot, String> {
    let game = game.lock().map_err(|_| "lock_poisoned".to_string())?;
    Ok(game.snapshot())
}

#[tauri::command]
fn make_move(
    from: String,
    to: String,
    promotion: Option<String>,
    game: State<'_, SharedGame>,
) -> Result<PositionSnapshot, String> {
    let mut game = game.lock().map_err(|_| "lock_poisoned".to_string())?;
    game.make_move(&from, &to, promotion.as_deref())
}

#[tauri::command]
fn reset_game(game: State<'_, SharedGame>) -> Result<PositionSnapshot, String> {
    let mut game = game.lock().map_err(|_| "lock_poisoned".to_string())?;
    game.reset();
    Ok(game.snapshot())
}

#[tauri::command]
fn list_engine_catalog() -> Result<Vec<engine::CatalogEngine>, String> {
    Ok(engine::list_catalog())
}

#[tauri::command]
fn list_installed_engines(app: AppHandle) -> Result<Vec<engine::EngineInfo>, String> {
    engine::list_installed(&app)
}

#[tauri::command]
async fn install_engine(
    app: AppHandle,
    engine_id: String,
) -> Result<engine::EngineInfo, String> {
    engine::install_engine(app, engine_id).await
}

#[tauri::command]
fn remove_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    engine::remove_engine(&app, engine_id)
}

#[tauri::command]
async fn list_database_catalog() -> Result<Vec<db::CatalogDatabase>, String> {
    db::fetch_catalog().await
}

#[tauri::command]
fn list_installed_databases(app: AppHandle) -> Result<Vec<db::DatabaseInfo>, String> {
    db::list_installed(&app)
}

#[tauri::command]
async fn install_database(
    app: AppHandle,
    catalog_id: String,
) -> Result<db::DatabaseInfo, String> {
    db::install_database(app, catalog_id).await
}

#[tauri::command]
fn remove_database(app: AppHandle, db_id: String) -> Result<(), String> {
    db::remove_database(&app, db_id)
}

#[tauri::command]
fn list_database_games(
    app: AppHandle,
    db_id: String,
    offset: u64,
    limit: u64,
) -> Result<db::GamesPage, String> {
    db::list_games(&app, db_id, offset, limit)
}

#[tauri::command]
fn cancel_download(
    download_id: String,
    canceller: State<'_, DownloadCanceller>,
) -> Result<bool, String> {
    canceller.request_cancel(&download_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(Game::new()) as SharedGame)
        .manage(DownloadCanceller::new())
        .invoke_handler(tauri::generate_handler![
            get_position,
            make_move,
            reset_game,
            list_engine_catalog,
            list_installed_engines,
            install_engine,
            remove_engine,
            list_database_catalog,
            list_installed_databases,
            install_database,
            remove_database,
            list_database_games,
            cancel_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

mod db;
mod engine;
mod fs;
mod game;
mod repertoire;

use std::sync::{Arc, Mutex};

use engine::{
    AnalysisManager, AnalysisSettings, CreateProfileInput, UpdateProfileInput,
};
use fs::DownloadCanceller;
use game::{Game, PositionSnapshot, SharedGame};
use repertoire::{
    BuildCanceller, CreateRepertoireInput, UpdateRepertoireMeta,
};
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
fn get_fen_position(fen: String) -> Result<PositionSnapshot, String> {
    let game = Game::from_fen(&fen)?;
    Ok(game.snapshot())
}

#[tauri::command]
fn apply_uci_move(fen: String, uci: String) -> Result<PositionSnapshot, String> {
    let mut game = Game::from_fen(&fen)?;
    game.play_uci(&uci)
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
fn list_analysis_profiles(app: AppHandle) -> Result<Vec<engine::AnalysisProfile>, String> {
    engine::list_profiles(&app)
}

#[tauri::command]
fn create_analysis_profile(
    app: AppHandle,
    input: CreateProfileInput,
) -> Result<engine::AnalysisProfile, String> {
    engine::create_profile(&app, input)
}

#[tauri::command]
fn update_analysis_profile(
    app: AppHandle,
    input: UpdateProfileInput,
) -> Result<engine::AnalysisProfile, String> {
    engine::update_profile(&app, input)
}

#[tauri::command]
fn delete_analysis_profile(app: AppHandle, profile_id: String) -> Result<(), String> {
    engine::delete_profile(&app, profile_id)
}

#[tauri::command]
fn max_engine_threads() -> u32 {
    engine::max_allowed_threads()
}

#[tauri::command]
fn start_analysis(
    app: AppHandle,
    analysis: State<'_, AnalysisManager>,
    engine_id: String,
    fen: String,
    settings: AnalysisSettings,
) -> Result<(), String> {
    analysis.start(app, engine_id, fen, settings)
}

#[tauri::command]
fn stop_analysis(analysis: State<'_, AnalysisManager>) -> Result<(), String> {
    analysis.stop()
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
async fn download_user_database(
    app: AppHandle,
    site: String,
    username: String,
) -> Result<db::DatabaseInfo, String> {
    db::download_user_database(app, site, username).await
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

#[tauri::command]
fn build_database_index(app: AppHandle, db_id: String) -> Result<(), String> {
    db::build_index(&app, &db_id)?;
    Ok(())
}

#[tauri::command]
fn database_index_status(app: AppHandle, db_id: String) -> Result<bool, String> {
    db::index_status(&app, &db_id)
}

#[tauri::command]
fn search_database_position(
    app: AppHandle,
    db_id: String,
    fen: String,
) -> Result<db::PositionSearchResult, String> {
    db::search_position(&app, &db_id, &fen)
}

#[tauri::command]
fn list_repertoires(
    app: AppHandle,
    canceller: State<'_, Arc<BuildCanceller>>,
) -> Result<Vec<repertoire::RepertoireSummary>, String> {
    repertoire::list_repertoires(&app, canceller.inner())
}

#[tauri::command]
fn get_repertoire(app: AppHandle, id: String) -> Result<repertoire::Repertoire, String> {
    repertoire::load_repertoire(&app, &id)
}

#[tauri::command]
fn create_repertoire(
    app: AppHandle,
    input: CreateRepertoireInput,
) -> Result<repertoire::Repertoire, String> {
    repertoire::create_repertoire(&app, input)
}

#[tauri::command]
fn update_repertoire(
    app: AppHandle,
    input: UpdateRepertoireMeta,
) -> Result<repertoire::Repertoire, String> {
    repertoire::update_repertoire_meta(&app, input)
}

#[tauri::command]
fn delete_repertoire(app: AppHandle, id: String) -> Result<(), String> {
    repertoire::delete_repertoire(&app, id)
}

#[tauri::command]
fn start_repertoire_build(
    app: AppHandle,
    canceller: State<'_, Arc<BuildCanceller>>,
    repertoire_id: String,
) -> Result<repertoire::Repertoire, String> {
    repertoire::spawn_build(app, Arc::clone(&canceller), repertoire_id, false)
}

#[tauri::command]
fn rebuild_repertoire(
    app: AppHandle,
    canceller: State<'_, Arc<BuildCanceller>>,
    repertoire_id: String,
) -> Result<repertoire::Repertoire, String> {
    repertoire::spawn_build(app, Arc::clone(&canceller), repertoire_id, true)
}

#[tauri::command]
fn continue_repertoire_build(
    app: AppHandle,
    canceller: State<'_, Arc<BuildCanceller>>,
    repertoire_id: String,
) -> Result<repertoire::Repertoire, String> {
    repertoire::spawn_continue(app, Arc::clone(&canceller), repertoire_id)
}

#[tauri::command]
fn stop_repertoire_build(canceller: State<'_, Arc<BuildCanceller>>) -> Result<(), String> {
    canceller.request_cancel()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let half = engine::max_allowed_threads() as usize;
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(half.max(1))
        .build_global();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(Game::new()) as SharedGame)
        .manage(DownloadCanceller::new())
        .manage(AnalysisManager::new())
        .manage(Arc::new(BuildCanceller::new()))
        .invoke_handler(tauri::generate_handler![
            get_position,
            make_move,
            reset_game,
            get_fen_position,
            apply_uci_move,
            list_engine_catalog,
            list_installed_engines,
            install_engine,
            remove_engine,
            list_analysis_profiles,
            create_analysis_profile,
            update_analysis_profile,
            delete_analysis_profile,
            max_engine_threads,
            start_analysis,
            stop_analysis,
            list_database_catalog,
            list_installed_databases,
            install_database,
            download_user_database,
            remove_database,
            list_database_games,
            cancel_download,
            build_database_index,
            database_index_status,
            search_database_position,
            list_repertoires,
            get_repertoire,
            create_repertoire,
            update_repertoire,
            delete_repertoire,
            start_repertoire_build,
            rebuild_repertoire,
            continue_repertoire_build,
            stop_repertoire_build,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

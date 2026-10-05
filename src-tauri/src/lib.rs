mod engine;
mod game;

use std::sync::Mutex;

use game::{Game, PositionSnapshot, SharedGame};
use tauri::State;

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
fn list_installed_engines() -> Result<Vec<engine::EngineInfo>, String> {
    engine::EngineManager::list_installed()
}

#[tauri::command]
fn install_engine(engine_id: String) -> Result<(), String> {
    engine::EngineManager::install(engine_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(Game::new()) as SharedGame)
        .invoke_handler(tauri::generate_handler![
            get_position,
            make_move,
            reset_game,
            list_installed_engines,
            install_engine,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

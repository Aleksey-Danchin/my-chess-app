use serde::Serialize;
use tauri::AppHandle;

use super::install::find_installed;
use super::result_col::{result_code, result_display};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub id: i64,
    pub white: String,
    pub black: String,
    pub result: String,
    pub date: Option<String>,
    pub event: Option<String>,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesPage {
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
    pub games: Vec<GameRow>,
}

pub fn list_games(
    app: &AppHandle,
    db_id: String,
    offset: u64,
    limit: u64,
) -> Result<GamesPage, String> {
    let limit = limit.clamp(1, 200);
    let record = find_installed(app, &db_id)?;

    let conn = rusqlite::Connection::open(&record.path)
        .map_err(|e| format!("open_db: {e}"))?;

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM Games", [], |row| row.get::<_, i64>(0))
        .map_err(|e| format!("count_games: {e}"))? as u64;

    let mut stmt = conn
        .prepare(
            "SELECT
                g.ID,
                COALESCE(w.Name, 'Unknown'),
                COALESCE(b.Name, 'Unknown'),
                g.Result,
                g.Date,
                e.Name,
                g.WhiteElo,
                g.BlackElo
             FROM Games g
             LEFT JOIN Players w ON g.WhiteID = w.ID
             LEFT JOIN Players b ON g.BlackID = b.ID
             LEFT JOIN Events e ON g.EventID = e.ID
             ORDER BY g.ID DESC
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| format!("prepare_games: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![limit as i64, offset as i64], |row| {
            let code = result_code(row.get_ref(3)?);
            Ok(GameRow {
                id: row.get(0)?,
                white: row.get(1)?,
                black: row.get(2)?,
                result: result_display(code),
                date: row.get(4)?,
                event: row.get(5)?,
                white_elo: row.get(6)?,
                black_elo: row.get(7)?,
            })
        })
        .map_err(|e| format!("query_games: {e}"))?;

    let mut games = Vec::new();
    for row in rows {
        games.push(row.map_err(|e| format!("game_row: {e}"))?);
    }

    Ok(GamesPage {
        total,
        offset,
        limit,
        games,
    })
}

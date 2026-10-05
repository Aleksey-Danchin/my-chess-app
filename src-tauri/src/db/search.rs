use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use rayon::prelude::*;
use serde::Serialize;
use shakmaty::{Chess, Color, Position};
use tauri::AppHandle;

use super::encoding::{
    chess_from_fen, fen_key, find_next_move_after_position_pruned, move_to_uci, position_matches,
};
use super::install::find_installed;
use super::prune::{
    game_can_reach_target, get_material_count, get_pawn_home, MaterialCount,
};
use super::search_index::{load_index, IndexGame};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveStat {
    pub uci: String,
    pub san: String,
    pub games: u64,
    pub white: u64,
    pub draw: u64,
    pub black: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionSearchResult {
    pub fen: String,
    pub fen_key: String,
    pub total_games: u64,
    pub moves: Vec<MoveStat>,
}

struct Acc {
    white: u64,
    draw: u64,
    black: u64,
    san: String,
}

type CacheKey = (String, String); // (db_id, fen_key)

fn search_cache() -> &'static Mutex<HashMap<CacheKey, PositionSearchResult>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, PositionSearchResult>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const CACHE_CAP: usize = 8_192;

pub fn clear_search_cache_for_db(db_id: &str) {
    if let Ok(mut g) = search_cache().lock() {
        g.retain(|(id, _), _| id != db_id);
    }
}

pub fn search_position(app: &AppHandle, db_id: &str, fen: &str) -> Result<PositionSearchResult, String> {
    search_position_with_progress(app, db_id, fen, None, |_, _| {})
}

/// Full index scan with reachability pruning. `on_progress(done, total)` may be called from
/// rayon workers — must be Sync. Pass `cancel` to abort early (returns `"cancelled"`).
pub fn search_position_with_progress<F>(
    app: &AppHandle,
    db_id: &str,
    fen: &str,
    cancel: Option<&AtomicBool>,
    on_progress: F,
) -> Result<PositionSearchResult, String>
where
    F: Fn(u64, u64) + Sync,
{
    let target = chess_from_fen(fen)?;
    let key = fen_key(&target);

    if cancel.is_none() {
        if let Ok(g) = search_cache().lock() {
            if let Some(hit) = g.get(&(db_id.to_string(), key.clone())) {
                return Ok(hit.clone());
            }
        }
    }

    let record = find_installed(app, db_id)?;
    let db_path = PathBuf::from(&record.path);
    let index = load_index(&db_path)?;

    let total = index.games.len() as u64;
    on_progress(0, total);

    let target_material = get_material_count(target.board());
    let target_pawn_home = get_pawn_home(target.board());
    let prune = Some((target_material, target_pawn_home));

    let start_default = Chess::default();
    let map: Mutex<HashMap<String, Acc>> = Mutex::new(HashMap::new());
    let matched = Mutex::new(0u64);
    let done = AtomicU64::new(0);
    let last_emit = AtomicU64::new(0);

    struct ScanCancelled;

    let scan = index
        .games
        .par_iter()
        .try_for_each(|game: &IndexGame| -> Result<(), ScanCancelled> {
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(ScanCancelled);
            }

            // End-of-game prune: skip games that cannot have reached the target.
            if game.pawn_home != 0 || game.white_material != 0 || game.black_material != 0 {
                let end_mat = MaterialCount {
                    white: game.white_material,
                    black: game.black_material,
                };
                if !game_can_reach_target(end_mat, game.pawn_home, &target) {
                    report_progress(&done, &last_emit, total, &on_progress);
                    return Ok(());
                }
            }

            let start = if let Some(ref f) = game.fen {
                match chess_from_fen(f) {
                    Ok(c) => c,
                    Err(_) => {
                        report_progress(&done, &last_emit, total, &on_progress);
                        return Ok(());
                    }
                }
            } else {
                start_default.clone()
            };

            match find_next_move_after_position_pruned(&game.moves, start, &target, prune) {
                Ok(Some((uci, san))) => {
                    if let Ok(mut m) = matched.lock() {
                        *m += 1;
                    }
                    if let Ok(mut map) = map.lock() {
                        let entry = map.entry(uci).or_insert(Acc {
                            white: 0,
                            draw: 0,
                            black: 0,
                            san,
                        });
                        match game.result {
                            1 => entry.white += 1,
                            2 => entry.black += 1,
                            3 => entry.draw += 1,
                            _ => {}
                        }
                    }
                }
                Ok(None) | Err(_) => {}
            }
            report_progress(&done, &last_emit, total, &on_progress);
            Ok(())
        });

    if scan.is_err() || cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("cancelled".into());
    }

    on_progress(total, total);

    let hit_total = *matched.lock().map_err(|_| "lock".to_string())?;
    let map = map.into_inner().map_err(|_| "lock".to_string())?;

    let mut moves: Vec<MoveStat> = map
        .into_iter()
        .map(|(uci, acc)| {
            let games = acc.white + acc.draw + acc.black;
            MoveStat {
                uci,
                san: acc.san,
                games,
                white: acc.white,
                draw: acc.draw,
                black: acc.black,
                percent: if hit_total > 0 {
                    (games as f64) * 100.0 / hit_total as f64
                } else {
                    0.0
                },
            }
        })
        .collect();

    moves.sort_by(|a, b| b.games.cmp(&a.games));

    let result = PositionSearchResult {
        fen: fen.to_string(),
        fen_key: key.clone(),
        total_games: hit_total,
        moves,
    };

    if let Ok(mut g) = search_cache().lock() {
        if g.len() >= CACHE_CAP {
            // Drop arbitrary half when full — simple size bound.
            let drop_n = CACHE_CAP / 2;
            let keys: Vec<_> = g.keys().take(drop_n).cloned().collect();
            for k in keys {
                g.remove(&k);
            }
        }
        g.insert((db_id.to_string(), key), result.clone());
    }

    Ok(result)
}

fn report_progress<F: Fn(u64, u64)>(
    done: &AtomicU64,
    last_emit: &AtomicU64,
    total: u64,
    on_progress: &F,
) {
    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
    let prev = last_emit.load(Ordering::Relaxed);
    if n == total || n.saturating_sub(prev) >= 50_000 {
        if last_emit
            .compare_exchange(prev, n, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            on_progress(n, total);
        }
    }
}

/// Apply a UCI move to a FEN, return new chess + fen_key.
pub fn play_uci_on_fen(fen: &str, uci: &str) -> Result<(Chess, String), String> {
    let mut chess = chess_from_fen(fen)?;
    let m = parse_uci_move(&chess, uci)?;
    chess = chess
        .play(m)
        .map_err(|_| format!("illegal_uci:{uci}"))?;
    let key = fen_key(&chess);
    Ok((chess, key))
}

fn parse_uci_move(chess: &Chess, uci: &str) -> Result<shakmaty::Move, String> {
    use shakmaty::{Role, Square};
    if uci.len() < 4 {
        return Err(format!("bad_uci:{uci}"));
    }
    let from = Square::from_ascii(uci[0..2].as_bytes()).map_err(|_| format!("bad_from:{uci}"))?;
    let to = Square::from_ascii(uci[2..4].as_bytes()).map_err(|_| format!("bad_to:{uci}"))?;
    let promo = if uci.len() >= 5 {
        Role::from_char(uci.chars().nth(4).unwrap_or(' '))
    } else {
        None
    };

    for m in chess.legal_moves() {
        if move_to_uci(&m) == uci {
            return Ok(m);
        }
        match &m {
            shakmaty::Move::Normal {
                from: f,
                to: t,
                promotion,
                ..
            } if *f == from && *t == to && *promotion == promo => {
                return Ok(m);
            }
            shakmaty::Move::EnPassant { from: f, to: t } if *f == from && *t == to => {
                return Ok(m);
            }
            shakmaty::Move::Castle { king, .. } if *king == from => {
                if move_to_uci(&m) == uci {
                    return Ok(m);
                }
            }
            _ => {}
        }
    }
    let _ = Color::White;
    let _ = position_matches;
    Err(format!("uci_not_legal:{uci}"))
}

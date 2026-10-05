//! Compact search index beside `.db3` (`.mcidx`). Stores move blobs + prune hints for position scan.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::install::find_installed;
use super::result_col::result_code;

const MAGIC: &[u8; 5] = b"MCIDX";
/// v2 adds end-of-game material + pawn-home for reachability pruning.
pub const VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexGame {
    pub id: i64,
    /// 1 = white win, 2 = black win, 3 = draw, 0 = other
    pub result: u8,
    pub fen: Option<String>,
    pub moves: Vec<u8>,
    /// Bitmask of pawns still on home ranks at end of game (0 = unknown / no prune).
    pub pawn_home: u16,
    /// White material at end of game (0 = unknown / no prune).
    pub white_material: u8,
    /// Black material at end of game (0 = unknown / no prune).
    pub black_material: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    pub version: u32,
    pub games: Vec<IndexGame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub db_id: String,
    pub done: u64,
    pub total: u64,
}

struct CachedIndex {
    mtime: SystemTime,
    index: Arc<SearchIndex>,
}

fn index_cache() -> &'static Mutex<HashMap<PathBuf, CachedIndex>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedIndex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn clear_index_cache_for_db(db_path: &Path) {
    let path = index_path_for_db(db_path);
    if let Ok(mut g) = index_cache().lock() {
        g.remove(&path);
    }
}

pub fn index_path_for_db(db_path: &Path) -> PathBuf {
    db_path.with_extension("mcidx")
}

pub fn index_exists(db_path: &Path) -> bool {
    index_path_for_db(db_path).is_file()
}

/// Read only magic+version (9 bytes). Never deserializes the full index.
fn peek_index_version(index_path: &Path) -> Result<u32, String> {
    let mut file = File::open(index_path).map_err(|e| format!("open_index: {e}"))?;
    let mut magic = [0u8; 5];
    file.read_exact(&mut magic)
        .map_err(|e| format!("read_magic: {e}"))?;
    if &magic != MAGIC {
        return Err("bad_index_magic".into());
    }
    let mut ver_buf = [0u8; 4];
    file.read_exact(&mut ver_buf)
        .map_err(|e| format!("read_ver: {e}"))?;
    Ok(u32::from_le_bytes(ver_buf))
}

/// True only if `.mcidx` exists and matches the current format version.
pub fn index_is_current(db_path: &Path) -> bool {
    let path = index_path_for_db(db_path);
    if !path.is_file() {
        return false;
    }
    matches!(peek_index_version(&path), Ok(v) if v == VERSION)
}

fn read_index_file(path: &Path) -> Result<SearchIndex, String> {
    let file = File::open(path).map_err(|e| format!("open_index: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 5];
    reader
        .read_exact(&mut magic)
        .map_err(|e| format!("read_magic: {e}"))?;
    if &magic != MAGIC {
        return Err("bad_index_magic".into());
    }
    let mut ver_buf = [0u8; 4];
    reader
        .read_exact(&mut ver_buf)
        .map_err(|e| format!("read_ver: {e}"))?;
    let version = u32::from_le_bytes(ver_buf);
    if version != VERSION {
        return Err(format!(
            "unsupported_index_version:{version} (rebuild index)"
        ));
    }
    bincode::deserialize_from(reader).map_err(|e| format!("decode_index: {e}"))
}

/// Load index into memory (cached by path + mtime). First load of a large DB is heavy;
/// subsequent searches reuse the Arc.
pub fn load_index(db_path: &Path) -> Result<Arc<SearchIndex>, String> {
    let path = index_path_for_db(db_path);
    if !path.is_file() {
        return Err("index_missing".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| format!("index_meta: {e}"))?;
    let mtime = meta
        .modified()
        .map_err(|e| format!("index_mtime: {e}"))?;

    if let Ok(g) = index_cache().lock() {
        if let Some(hit) = g.get(&path) {
            if hit.mtime == mtime {
                return Ok(Arc::clone(&hit.index));
            }
        }
    }

    let index = Arc::new(read_index_file(&path)?);

    if let Ok(mut g) = index_cache().lock() {
        g.insert(
            path,
            CachedIndex {
                mtime,
                index: Arc::clone(&index),
            },
        );
    }

    Ok(index)
}

fn read_pawn_home(v: ValueRef<'_>) -> Option<u16> {
    match v {
        ValueRef::Integer(i) if (0..=u16::MAX as i64).contains(&i) => Some(i as u16),
        ValueRef::Blob(b) if b.len() >= 2 => Some(u16::from_le_bytes([b[0], b[1]])),
        _ => None,
    }
}

fn read_material_u8(v: ValueRef<'_>) -> Option<u8> {
    match v {
        ValueRef::Integer(i) if (0..=255).contains(&i) => Some(i as u8),
        _ => None,
    }
}

pub fn build_index(app: &AppHandle, db_id: &str) -> Result<PathBuf, String> {
    let record = find_installed(app, db_id)?;
    let db_path = PathBuf::from(&record.path);
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| format!("open_db: {e}"))?;

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM Games", [], |row| row.get::<_, i64>(0))
        .map_err(|e| format!("count: {e}"))? as u64;

    let mut stmt = conn
        .prepare(
            "SELECT ID, Result, FEN, Moves, WhiteMaterial, BlackMaterial, PawnHome
             FROM Games ORDER BY ID",
        )
        .map_err(|e| format!("prepare: {e}"))?;

    let mut games = Vec::with_capacity(total.min(1_000_000) as usize);
    let mut done = 0u64;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                result_code(row.get_ref(1)?),
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                read_material_u8(row.get_ref(4)?),
                read_material_u8(row.get_ref(5)?),
                read_pawn_home(row.get_ref(6)?),
            ))
        })
        .map_err(|e| format!("query: {e}"))?;

    for row in rows {
        let (id, result, fen, moves, white_mat, black_mat, pawn_home) =
            row.map_err(|e| format!("row: {e}"))?;

        // Without pawn_home we cannot safely prune; zero fields disable pruning.
        let (white_material, black_material, pawn_home) = match (white_mat, black_mat, pawn_home) {
            (Some(w), Some(b), Some(p)) => (w, b, p),
            _ => (0, 0, 0),
        };

        games.push(IndexGame {
            id,
            result,
            fen,
            moves,
            pawn_home,
            white_material,
            black_material,
        });
        done += 1;
        if done % 5_000 == 0 || done == total {
            let _ = app.emit(
                "db-index-progress",
                IndexProgress {
                    db_id: db_id.to_string(),
                    done,
                    total,
                },
            );
        }
    }

    let index = SearchIndex {
        version: VERSION,
        games,
    };

    let out = index_path_for_db(&db_path);
    let file = File::create(&out).map_err(|e| format!("create_index: {e}"))?;
    let mut writer = BufWriter::new(file);
    use std::io::Write;
    writer
        .write_all(MAGIC)
        .map_err(|e| format!("write_magic: {e}"))?;
    writer
        .write_all(&VERSION.to_le_bytes())
        .map_err(|e| format!("write_ver: {e}"))?;
    bincode::serialize_into(&mut writer, &index).map_err(|e| format!("encode_index: {e}"))?;
    writer.flush().map_err(|e| format!("flush_index: {e}"))?;

    clear_index_cache_for_db(&db_path);
    if let Ok(meta) = std::fs::metadata(&out) {
        if let Ok(mtime) = meta.modified() {
            if let Ok(mut g) = index_cache().lock() {
                g.insert(
                    out.clone(),
                    CachedIndex {
                        mtime,
                        index: Arc::new(index),
                    },
                );
            }
        }
    }

    super::search::clear_search_cache_for_db(db_id);

    let _ = app.emit(
        "db-index-progress",
        IndexProgress {
            db_id: db_id.to_string(),
            done: total,
            total,
        },
    );

    Ok(out)
}

pub fn index_status(app: &AppHandle, db_id: &str) -> Result<bool, String> {
    let record = find_installed(app, db_id)?;
    Ok(index_is_current(Path::new(&record.path)))
}

//! Create a `.db3` from a PGN stream (en-croissant-compatible schema + move encoding).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use pgn_reader::{Reader, RawTag, SanPlus, Skip, Visitor};
use rusqlite::{params, Connection};
use shakmaty::{
    fen::Fen, CastlingMode, Chess, FromSetup, Position, PositionError,
};

use super::encoding::encode_move;
use super::prune::{get_material_count, get_pawn_home};
use super::result_col::result_code_from_pgn;
use crate::fs::cancel::is_cancelled;

const CREATE_SQL: &str = r#"
CREATE TABLE Info (
    Name TEXT UNIQUE NOT NULL,
    Value TEXT
);

CREATE TABLE Events (
    ID INTEGER PRIMARY KEY AUTOINCREMENT,
    Name TEXT UNIQUE
);

CREATE TABLE Sites (
    ID INTEGER PRIMARY KEY AUTOINCREMENT,
    Name TEXT UNIQUE
);

CREATE TABLE Players (
    ID INTEGER PRIMARY KEY,
    Name TEXT UNIQUE,
    Elo INTEGER
);

CREATE TABLE Games (
    ID INTEGER PRIMARY KEY AUTOINCREMENT,
    EventID INTEGER,
    SiteID INTEGER,
    Date TEXT,
    UTCTime TEXT,
    Round INTEGER,
    WhiteID INTEGER,
    WhiteElo INTEGER,
    BlackID INTEGER,
    BlackElo INTEGER,
    WhiteMaterial INTEGER,
    BlackMaterial INTEGER,
    Result INTEGER,
    TimeControl TEXT,
    ECO TEXT,
    PlyCount INTEGER,
    FEN TEXT,
    Moves BLOB,
    PawnHome INTEGER,
    FOREIGN KEY(EventID) REFERENCES Events,
    FOREIGN KEY(SiteID) REFERENCES Sites,
    FOREIGN KEY(WhiteID) REFERENCES Players,
    FOREIGN KEY(BlackID) REFERENCES Players
);

INSERT INTO Players (ID, Name, Elo) VALUES (0, 'Unknown', NULL);
INSERT INTO Events (ID, Name) VALUES (0, 'Unknown');
INSERT INTO Sites (ID, Name) VALUES (0, 'Unknown');
"#;

#[derive(Default)]
struct Tags {
    event: Option<String>,
    site: Option<String>,
    date: Option<String>,
    utc_time: Option<String>,
    round: Option<String>,
    white: Option<String>,
    black: Option<String>,
    white_elo: Option<i64>,
    black_elo: Option<i64>,
    result: Option<String>,
    time_control: Option<String>,
    eco: Option<String>,
    fen: Option<String>,
    start: Chess,
    skip: bool,
}

struct Frame {
    position: Chess,
}

struct Movetext {
    tags: Tags,
    frames: Vec<Frame>,
    moves: Vec<u8>,
    skip: bool,
    /// Lowest material seen on the mainline (promotions can raise the final count).
    min_white_material: u8,
    min_black_material: u8,
}

struct ParsedGame {
    tags: Tags,
    moves: Vec<u8>,
    white_material: u8,
    black_material: u8,
    pawn_home: u16,
}

struct Importer {
    cancel: Option<Arc<AtomicBool>>,
}

impl Visitor for Importer {
    type Tags = Tags;
    type Movetext = Movetext;
    type Output = Option<ParsedGame>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        if self.cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
            return ControlFlow::Break(None);
        }
        ControlFlow::Continue(Tags::default())
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let text = value.decode_utf8_lossy().into_owned();
        match name {
            b"Event" => tags.event = Some(text),
            b"Site" => tags.site = Some(text),
            b"Date" | b"UTCDate" => {
                if tags.date.is_none() {
                    tags.date = Some(text);
                }
            }
            b"UTCTime" => tags.utc_time = Some(text),
            b"Round" => tags.round = Some(text),
            b"White" => tags.white = Some(text),
            b"Black" => tags.black = Some(text),
            b"WhiteElo" => {
                tags.white_elo = if text == "-" {
                    Some(0)
                } else {
                    text.parse().ok()
                };
            }
            b"BlackElo" => {
                tags.black_elo = if text == "-" {
                    Some(0)
                } else {
                    text.parse().ok()
                };
            }
            b"Result" => tags.result = Some(text),
            b"TimeControl" => tags.time_control = Some(text),
            b"ECO" => tags.eco = Some(text),
            b"FEN" => {
                if text
                    == "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
                {
                    tags.fen = None;
                } else {
                    match Fen::from_ascii(text.as_bytes()) {
                        Ok(fen) => {
                            let setup = fen.into_setup();
                            let mode = CastlingMode::detect(&setup);
                            match Chess::from_setup(setup, mode)
                                .or_else(PositionError::ignore_too_much_material)
                            {
                                Ok(pos) => {
                                    tags.fen = Some(text);
                                    tags.start = pos;
                                }
                                Err(_) => tags.skip = true,
                            }
                        }
                        Err(_) => tags.skip = true,
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn begin_movetext(
        &mut self,
        tags: Self::Tags,
    ) -> ControlFlow<Self::Output, Self::Movetext> {
        if tags.skip {
            return ControlFlow::Break(None);
        }
        let start = tags.start.clone();
        let mat = get_material_count(start.board());
        ControlFlow::Continue(Movetext {
            tags,
            frames: vec![Frame { position: start }],
            moves: Vec::new(),
            skip: false,
            min_white_material: mat.white,
            min_black_material: mat.black,
        })
    }

    fn san(
        &mut self,
        mt: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        if mt.skip {
            return ControlFlow::Continue(());
        }
        let Some(frame) = mt.frames.last_mut() else {
            mt.skip = true;
            return ControlFlow::Continue(());
        };
        match san_plus.san.to_move(&frame.position) {
            Ok(m) => {
                if let Some(byte) = encode_move(&m, &frame.position) {
                    mt.moves.push(byte);
                    frame.position.play_unchecked(m);
                    let mat = get_material_count(frame.position.board());
                    mt.min_white_material = mt.min_white_material.min(mat.white);
                    mt.min_black_material = mt.min_black_material.min(mat.black);
                } else {
                    mt.skip = true;
                }
            }
            Err(_) => mt.skip = true,
        }
        ControlFlow::Continue(())
    }

    fn begin_variation(
        &mut self,
        mt: &mut Self::Movetext,
    ) -> ControlFlow<Self::Output, Skip> {
        // Skip variations — keep mainline only for compact DBs.
        let _ = mt;
        ControlFlow::Continue(Skip(true))
    }

    fn end_game(&mut self, mt: Self::Movetext) -> Self::Output {
        if mt.skip {
            None
        } else {
            let final_pos = mt
                .frames
                .last()
                .map(|f| &f.position)
                .cloned()
                .unwrap_or_default();
            let final_mat = get_material_count(final_pos.board());
            Some(ParsedGame {
                tags: mt.tags,
                moves: mt.moves,
                white_material: mt.min_white_material.min(final_mat.white),
                black_material: mt.min_black_material.min(final_mat.black),
                pawn_home: get_pawn_home(final_pos.board()),
            })
        }
    }
}

struct NameCache {
    map: HashMap<String, i64>,
}

impl NameCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn player_id(&mut self, conn: &Connection, name: &str) -> Result<i64, String> {
        if let Some(&id) = self.map.get(name) {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO Players (Name) VALUES (?1) ON CONFLICT(Name) DO NOTHING",
            params![name],
        )
        .map_err(|e| format!("insert_player: {e}"))?;
        let id: i64 = conn
            .query_row(
                "SELECT ID FROM Players WHERE Name = ?1",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| format!("player_id: {e}"))?;
        self.map.insert(name.to_string(), id);
        Ok(id)
    }

    fn event_id(&mut self, conn: &Connection, name: &str) -> Result<i64, String> {
        let key = format!("e:{name}");
        if let Some(&id) = self.map.get(&key) {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO Events (Name) VALUES (?1) ON CONFLICT(Name) DO NOTHING",
            params![name],
        )
        .map_err(|e| format!("insert_event: {e}"))?;
        let id: i64 = conn
            .query_row(
                "SELECT ID FROM Events WHERE Name = ?1",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| format!("event_id: {e}"))?;
        self.map.insert(key, id);
        Ok(id)
    }

    fn site_id(&mut self, conn: &Connection, name: &str) -> Result<i64, String> {
        let key = format!("s:{name}");
        if let Some(&id) = self.map.get(&key) {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO Sites (Name) VALUES (?1) ON CONFLICT(Name) DO NOTHING",
            params![name],
        )
        .map_err(|e| format!("insert_site: {e}"))?;
        let id: i64 = conn
            .query_row(
                "SELECT ID FROM Sites WHERE Name = ?1",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| format!("site_id: {e}"))?;
        self.map.insert(key, id);
        Ok(id)
    }
}

fn insert_game(
    conn: &Connection,
    cache: &mut NameCache,
    game: &ParsedGame,
) -> Result<(), String> {
    let white_id = match &game.tags.white {
        Some(n) => cache.player_id(conn, n)?,
        None => 0,
    };
    let black_id = match &game.tags.black {
        Some(n) => cache.player_id(conn, n)?,
        None => 0,
    };
    let event_id = match &game.tags.event {
        Some(n) => cache.event_id(conn, n)?,
        None => 0,
    };
    let site_id = match &game.tags.site {
        Some(n) => cache.site_id(conn, n)?,
        None => 0,
    };
    let result = game
        .tags
        .result
        .as_deref()
        .map(result_code_from_pgn)
        .unwrap_or(0);
    let ply_count = game.moves.len() as i64;
    let round: Option<i64> = game
        .tags
        .round
        .as_deref()
        .and_then(|r| r.parse().ok());

    conn.execute(
        "INSERT INTO Games (
            EventID, SiteID, Date, UTCTime, Round,
            WhiteID, WhiteElo, BlackID, BlackElo,
            WhiteMaterial, BlackMaterial, Result,
            TimeControl, ECO, PlyCount, FEN, Moves, PawnHome
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9,
            ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18
         )",
        params![
            event_id,
            site_id,
            game.tags.date,
            game.tags.utc_time,
            round,
            white_id,
            game.tags.white_elo,
            black_id,
            game.tags.black_elo,
            game.white_material as i64,
            game.black_material as i64,
            result,
            game.tags.time_control,
            game.tags.eco,
            ply_count,
            game.tags.fen,
            game.moves.as_slice(),
            game.pawn_home as i64,
        ],
    )
    .map_err(|e| format!("insert_game: {e}"))?;
    Ok(())
}

/// Import all games from a PGN file into a new `.db3` at `db_path`.
/// Returns number of imported games.
pub fn convert_pgn_file(
    pgn_path: &Path,
    db_path: &Path,
    title: &str,
    cancel: Option<Arc<AtomicBool>>,
    mut on_progress: impl FnMut(u64),
) -> Result<u64, String> {
    if let Some(parent) = db_path.parent() {
        crate::fs::ensure_dir(parent)?;
    }
    if db_path.exists() {
        std::fs::remove_file(db_path).map_err(|e| format!("clean_db: {e}"))?;
    }

    let conn = Connection::open(db_path).map_err(|e| format!("open_db: {e}"))?;
    conn.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;")
        .map_err(|e| format!("pragma: {e}"))?;
    conn.execute_batch(CREATE_SQL)
        .map_err(|e| format!("create_schema: {e}"))?;
    conn.execute(
        "INSERT INTO Info (Name, Value) VALUES ('Title', ?1), ('Version', '1')",
        params![title],
    )
    .map_err(|e| format!("info: {e}"))?;

    let file = File::open(pgn_path).map_err(|e| format!("open_pgn: {e}"))?;
    let reader = BufReader::new(file);
    let mut pgn = Reader::new(reader);
    let mut importer = Importer {
        cancel: cancel.clone(),
    };
    let mut cache = NameCache::new();
    let mut imported = 0u64;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("begin_tx: {e}"))?;

    loop {
        if cancel.as_ref().is_some_and(|f| is_cancelled(f)) {
            drop(tx);
            let _ = std::fs::remove_file(db_path);
            return Err(crate::fs::cancel::CANCELLED.into());
        }
        match pgn.read_game(&mut importer) {
            Ok(Some(Some(game))) => {
                insert_game(&tx, &mut cache, &game)?;
                imported += 1;
                if imported % 200 == 0 {
                    on_progress(imported);
                }
            }
            Ok(Some(None)) => {}
            Ok(None) => break,
            Err(e) => {
                drop(tx);
                let _ = std::fs::remove_file(db_path);
                return Err(format!("pgn_parse: {e}"));
            }
        }
    }

    tx.commit().map_err(|e| format!("commit: {e}"))?;

    conn.execute(
        "INSERT INTO Info (Name, Value) VALUES ('GameCount', ?1)
         ON CONFLICT(Name) DO UPDATE SET Value = excluded.Value",
        params![imported.to_string()],
    )
    .map_err(|e| format!("game_count: {e}"))?;

    on_progress(imported);
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn imports_simple_pgn() {
        let mut pgn = NamedTempFile::new().unwrap();
        write!(
            pgn,
            r#"[Event "T"]
[Site "S"]
[Date "2024.01.01"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]
[WhiteElo "1500"]
[BlackElo "1400"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0
"#
        )
        .unwrap();
        let db = NamedTempFile::new().unwrap();
        let db_path = db.path().with_extension("db3");
        let n = convert_pgn_file(pgn.path(), &db_path, "Test", None, |_| {}).unwrap();
        assert_eq!(n, 1);
        let conn = Connection::open(&db_path).unwrap();
        let result: i64 = conn
            .query_row("SELECT Result FROM Games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 1);
        let moves: Vec<u8> = conn
            .query_row("SELECT Moves FROM Games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(moves.len(), 6);
        let (wm, bm, ph): (i64, i64, i64) = conn
            .query_row(
                "SELECT WhiteMaterial, BlackMaterial, PawnHome FROM Games",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(wm, 39);
        assert_eq!(bm, 39);
        assert_ne!(ph, 0);
        let _ = std::fs::remove_file(&db_path);
    }
}
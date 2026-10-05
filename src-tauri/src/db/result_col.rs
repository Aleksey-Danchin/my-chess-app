//! `Games.Result` is INTEGER in the schema, but some ready `.db3` files
//! (e.g. MillionBase) store PGN strings like `1-0` as TEXT. SQLite allows both.

use rusqlite::types::ValueRef;

/// Normalize DB value to: 1 = white, 2 = black, 3 = draw, 0 = other/`*`.
pub fn result_code(v: ValueRef<'_>) -> u8 {
    match v {
        ValueRef::Integer(i) => match i {
            1 => 1,
            2 => 2,
            3 => 3,
            _ => 0,
        },
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes).unwrap_or("").trim() {
            "1-0" => 1,
            "0-1" => 2,
            "1/2-1/2" | "½-½" => 3,
            _ => 0,
        },
        _ => 0,
    }
}

pub fn result_display(code: u8) -> String {
    match code {
        1 => "1-0".into(),
        2 => "0-1".into(),
        3 => "1/2-1/2".into(),
        _ => "*".into(),
    }
}

pub fn result_code_from_pgn(s: &str) -> i64 {
    match s.trim() {
        "1-0" => 1,
        "0-1" => 2,
        "1/2-1/2" | "½-½" => 3,
        _ => 0,
    }
}

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use shakmaty::{
    fen::Fen, CastlingSide, Chess, Color, EnPassantMode, Move, Position, Role, Square,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionSnapshot {
    pub fen: String,
    pub turn: String,
    pub dests: HashMap<String, Vec<String>>,
    pub last_move: Option<[String; 2]>,
    pub check: bool,
    pub status: String,
    /// True when the attempted move needs a promotion piece choice.
    pub needs_promotion: bool,
}

pub struct Game {
    position: Chess,
    last_move: Option<[Square; 2]>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Self {
            position: Chess::default(),
            last_move: None,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn snapshot(&self) -> PositionSnapshot {
        self.snapshot_with_promotion(false)
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        use shakmaty::{fen::Fen, CastlingMode, FromSetup, PositionError};
        let parsed: Fen = fen.parse().map_err(|e| format!("invalid_fen: {e}"))?;
        let setup = parsed.into_setup();
        let mode = CastlingMode::detect(&setup);
        let position = Chess::from_setup(setup, mode)
            .or_else(PositionError::ignore_too_much_material)
            .map_err(|e| format!("fen_setup: {e}"))?;
        Ok(Self {
            position,
            last_move: None,
        })
    }

    pub fn play_uci(&mut self, uci: &str) -> Result<PositionSnapshot, String> {
        let from = &uci.get(0..2).ok_or_else(|| "bad_uci".to_string())?;
        let to = &uci.get(2..4).ok_or_else(|| "bad_uci".to_string())?;
        let promo = uci.chars().nth(4).map(|c| c.to_string());
        self.make_move(from, to, promo.as_deref())
    }

    fn snapshot_with_promotion(&self, needs_promotion: bool) -> PositionSnapshot {
        let fen = Fen::from_position(&self.position, EnPassantMode::Legal).to_string();
        let turn = match self.position.turn() {
            Color::White => "white",
            Color::Black => "black",
        }
        .to_string();

        let status = if self.position.is_checkmate() {
            "checkmate"
        } else if self.position.is_stalemate() {
            "stalemate"
        } else {
            "playing"
        }
        .to_string();

        let last_move = self
            .last_move
            .map(|[from, to]| [square_key(from), square_key(to)]);

        PositionSnapshot {
            fen,
            turn,
            dests: compute_dests(&self.position),
            last_move,
            check: self.position.is_check(),
            status,
            needs_promotion,
        }
    }

    pub fn make_move(
        &mut self,
        from: &str,
        to: &str,
        promotion: Option<&str>,
    ) -> Result<PositionSnapshot, String> {
        let from_sq = parse_square(from)?;
        let to_sq = parse_square(to)?;
        let promo_role = parse_promotion(promotion)?;

        let candidates: Vec<Move> = self
            .position
            .legal_moves()
            .into_iter()
            .filter(|m| matches_user_move(m, from_sq, to_sq))
            .collect();

        if candidates.is_empty() {
            return Err("illegal_move".into());
        }

        let needs_promo = candidates.iter().any(|m| m.promotion().is_some());
        if needs_promo && promo_role.is_none() {
            return Ok(self.snapshot_with_promotion(true));
        }

        let chosen = candidates
            .into_iter()
            .find(|m| m.promotion() == promo_role)
            .ok_or_else(|| "illegal_move".to_string())?;

        let display_to = move_display_to(&chosen);
        self.position = self
            .position
            .clone()
            .play(chosen)
            .map_err(|_| "illegal_move".to_string())?;
        self.last_move = Some([from_sq, display_to]);

        Ok(self.snapshot())
    }
}

pub type SharedGame = Mutex<Game>;

fn compute_dests(position: &Chess) -> HashMap<String, Vec<String>> {
    let mut dests: HashMap<String, Vec<String>> = HashMap::new();
    for m in position.legal_moves() {
        if let Some(from) = m.from() {
            let targets = move_dest_squares(&m);
            let entry = dests.entry(square_key(from)).or_default();
            for t in targets {
                entry.push(square_key(t));
            }
        }
    }
    for list in dests.values_mut() {
        list.sort();
        list.dedup();
    }
    dests
}

/// Squares Chessground may send as the move destination.
fn move_dest_squares(m: &Move) -> Vec<Square> {
    match m {
        Move::Castle { king, rook } => {
            let side = CastlingSide::from_king_side(*king < *rook);
            vec![
                Square::from_coords(side.king_to_file(), king.rank()),
                *rook,
            ]
        }
        _ => vec![m.to()],
    }
}

fn matches_user_move(m: &Move, from: Square, to: Square) -> bool {
    m.from() == Some(from) && move_dest_squares(m).contains(&to)
}

fn move_display_to(m: &Move) -> Square {
    match m {
        Move::Castle { king, rook } => {
            let side = CastlingSide::from_king_side(*king < *rook);
            Square::from_coords(side.king_to_file(), king.rank())
        }
        _ => m.to(),
    }
}

fn square_key(sq: Square) -> String {
    sq.to_string()
}

fn parse_square(s: &str) -> Result<Square, String> {
    Square::from_ascii(s.as_bytes()).map_err(|_| format!("invalid_square:{s}"))
}

fn parse_promotion(promotion: Option<&str>) -> Result<Option<Role>, String> {
    match promotion {
        None => Ok(None),
        Some(p) => {
            let ch = p
                .chars()
                .next()
                .ok_or_else(|| format!("invalid_promotion:{p}"))?;
            let role = Role::from_char(ch).ok_or_else(|| format!("invalid_promotion:{p}"))?;
            if matches!(
                role,
                Role::Queen | Role::Rook | Role::Bishop | Role::Knight
            ) {
                Ok(Some(role))
            } else {
                Err(format!("invalid_promotion:{p}"))
            }
        }
    }
}

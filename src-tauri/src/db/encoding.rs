//! en-croissant-compatible move blob encoding.
//! Move byte = index into `chess.legal_moves()`. Markers skip variations/comments.

use shakmaty::{san::SanPlus, Chess, Move, Position};

pub const VARIATION_START_MARKER: u8 = 255;
pub const VARIATION_END_MARKER: u8 = 254;
pub const COMMENT_MARKER: u8 = 253;
pub const NAG_MARKER: u8 = 252;

pub fn decode_move(byte: u8, chess: &Chess) -> Option<Move> {
    chess.legal_moves().get(byte as usize).cloned()
}

pub fn encode_move(m: &Move, chess: &Chess) -> Option<u8> {
    chess
        .legal_moves()
        .iter()
        .position(|x| x == m)
        .map(|i| i as u8)
}

pub struct MainlineMoveBytesIter<'a> {
    bytes: &'a [u8],
    cursor: usize,
    variation_depth: usize,
}

impl<'a> MainlineMoveBytesIter<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            cursor: 0,
            variation_depth: 0,
        }
    }
}

impl Iterator for MainlineMoveBytesIter<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor < self.bytes.len() {
            let byte = self.bytes[self.cursor];
            self.cursor += 1;

            match byte {
                VARIATION_START_MARKER => {
                    self.variation_depth = self.variation_depth.saturating_add(1);
                }
                VARIATION_END_MARKER => {
                    self.variation_depth = self.variation_depth.saturating_sub(1);
                }
                COMMENT_MARKER | NAG_MARKER => {
                    if self.cursor + 2 > self.bytes.len() {
                        return None;
                    }
                    let len =
                        u16::from_le_bytes([self.bytes[self.cursor], self.bytes[self.cursor + 1]])
                            as usize;
                    self.cursor += 2;
                    self.cursor = self.cursor.saturating_add(len).min(self.bytes.len());
                }
                _ if self.variation_depth == 0 => return Some(byte),
                _ => {}
            }
        }
        None
    }
}

pub fn iter_mainline_move_bytes(bytes: &[u8]) -> MainlineMoveBytesIter<'_> {
    MainlineMoveBytesIter::new(bytes)
}

/// Replay mainline until `target` matches (board + turn), then return next move UCI/SAN if any.
#[allow(dead_code)]
pub fn find_next_move_after_position(
    moves_bytes: &[u8],
    start: Chess,
    target: &Chess,
) -> Result<Option<(String, String)>, String> {
    find_next_move_after_position_pruned(moves_bytes, start, target, None)
}

/// Same as [`find_next_move_after_position`], but may abort early when `prune` proves the
/// target is unreachable after an irreversible move (capture / pawn / promotion).
pub fn find_next_move_after_position_pruned(
    moves_bytes: &[u8],
    start: Chess,
    target: &Chess,
    prune: Option<(super::prune::MaterialCount, u16)>,
) -> Result<Option<(String, String)>, String> {
    let mut chess = start;
    for byte in iter_mainline_move_bytes(moves_bytes) {
        if position_matches(&chess, target) {
            let Some(m) = decode_move(byte, &chess) else {
                return Ok(None);
            };
            let san = SanPlus::from_move(chess.clone(), m.clone()).to_string();
            let uci = move_to_uci(&m);
            return Ok(Some((uci, san)));
        }
        let Some(m) = decode_move(byte, &chess) else {
            break;
        };
        let irreversible = m.is_capture() || m.is_promotion() || m.role() == shakmaty::Role::Pawn;
        chess = chess.play(m).map_err(|_| "illegal_encoded_move".to_string())?;
        if irreversible {
            if let Some((tm, tp)) = prune {
                if !super::prune::current_can_reach_target(&chess, tm, tp) {
                    return Ok(None);
                }
            }
        }
    }
    if position_matches(&chess, target) {
        return Ok(None); // game ended in target position
    }
    Ok(None)
}

pub fn position_matches(a: &Chess, b: &Chess) -> bool {
    a.turn() == b.turn() && a.board() == b.board()
}

pub fn move_to_uci(m: &Move) -> String {
    match m {
        Move::Normal {
            from,
            to,
            promotion,
            ..
        } => {
            let mut s = format!("{from}{to}");
            if let Some(role) = promotion {
                s.push(role.char());
            }
            s
        }
        Move::EnPassant { from, to } => format!("{from}{to}"),
        Move::Castle { king, rook } => {
            // UCI castling is king destination (e1g1 / e1c1 style via king.to after castling)
            // Shakmaty Castle uses king and rook squares; compute king destination file.
            use shakmaty::{CastlingSide, Square};
            let side = CastlingSide::from_king_side(*king < *rook);
            let to = Square::from_coords(side.king_to_file(), king.rank());
            format!("{king}{to}")
        }
        Move::Put { role, to } => format!("{}@{to}", role.char()),
    }
}

/// FEN key without halfmove/fullmove clocks.
pub fn fen_key(chess: &Chess) -> String {
    use shakmaty::fen::Fen;
    use shakmaty::EnPassantMode;
    let fen = Fen::from_position(chess, EnPassantMode::Legal).to_string();
    let parts: Vec<&str> = fen.split_whitespace().collect();
    if parts.len() >= 4 {
        format!("{} {} {} {}", parts[0], parts[1], parts[2], parts[3])
    } else {
        fen
    }
}

pub fn chess_from_fen(fen: &str) -> Result<Chess, String> {
    use shakmaty::{fen::Fen, CastlingMode, FromSetup, PositionError};
    let fen: Fen = fen
        .parse()
        .map_err(|e| format!("invalid_fen: {e}"))?;
    let setup = fen.into_setup();
    let mode = CastlingMode::detect(&setup);
    Chess::from_setup(setup, mode)
        .or_else(PositionError::ignore_too_much_material)
        .map_err(|e| format!("fen_setup: {e}"))
}

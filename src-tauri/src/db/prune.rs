//! Reachability pruning for position search (en-croissant style).
//!
//! A game can only reach a target position if:
//! - end-of-game material ≤ material in the target (captures only decrease material);
//! - every pawn still on its home rank at the end of the game was also home in the target
//!   (pawns never return to the 2nd/7th rank start files).

use shakmaty::{Board, Color, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialCount {
    pub white: u8,
    pub black: u8,
}

pub fn get_material_count(board: &Board) -> MaterialCount {
    let mat = board.material();
    MaterialCount {
        white: score_side(&mat.white),
        black: score_side(&mat.black),
    }
}

fn score_side(side: &shakmaty::ByRole<u8>) -> u8 {
    side.pawn
        .saturating_add(side.knight.saturating_mul(3))
        .saturating_add(side.bishop.saturating_mul(3))
        .saturating_add(side.rook.saturating_mul(5))
        .saturating_add(side.queen.saturating_mul(9))
}

/// Bitmask of white pawns on the 2nd rank (low 8 bits) and black pawns on the 7th (high 8 bits).
pub fn get_pawn_home(board: &Board) -> u16 {
    let white_pawns = board.by_piece(Color::White.pawn());
    let black_pawns = board.by_piece(Color::Black.pawn());
    let second_rank = (white_pawns.0 >> 8) as u8;
    let seventh_rank = (black_pawns.0 >> 48) as u8;
    u16::from(second_rank) | (u16::from(seventh_rank) << 8)
}

/// `end` bits must be a subset of `pos` bits: pawns still home at the later position
/// must also have been home earlier.
#[inline]
pub fn is_end_reachable(end: u16, pos: u16) -> bool {
    end & !pos == 0
}

#[inline]
pub fn is_material_reachable(end: &MaterialCount, pos: &MaterialCount) -> bool {
    end.white <= pos.white && end.black <= pos.black
}

/// Can this game (end material + end pawn-home) have passed through `target`?
#[inline]
pub fn game_can_reach_target(
    end_material: MaterialCount,
    end_pawn_home: u16,
    target: &impl Position,
) -> bool {
    let board = target.board();
    let target_material = get_material_count(board);
    let target_pawn_home = get_pawn_home(board);
    is_end_reachable(end_pawn_home, target_pawn_home)
        && is_material_reachable(&end_material, &target_material)
}

/// After an irreversible move, can we still reach the target from the current board?
#[inline]
pub fn current_can_reach_target(
    current: &impl Position,
    target_material: MaterialCount,
    target_pawn_home: u16,
) -> bool {
    let board = current.board();
    is_end_reachable(target_pawn_home, get_pawn_home(board))
        && is_material_reachable(&target_material, &get_material_count(board))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Chess;

    #[test]
    fn starting_pawn_home_is_full() {
        assert_eq!(get_pawn_home(Chess::default().board()), 0xffff);
    }

    #[test]
    fn starting_material_is_39() {
        let m = get_material_count(Chess::default().board());
        assert_eq!(m.white, 39);
        assert_eq!(m.black, 39);
    }
}

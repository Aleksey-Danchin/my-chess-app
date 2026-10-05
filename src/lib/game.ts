import { invoke } from "@tauri-apps/api/core";

export type Turn = "white" | "black";
export type GameStatus = "playing" | "checkmate" | "stalemate";

export type PositionSnapshot = {
  fen: string;
  turn: Turn;
  dests: Record<string, string[]>;
  lastMove: [string, string] | null;
  check: boolean;
  status: GameStatus;
  needsPromotion: boolean;
};

export type PromotionPiece = "q" | "r" | "b" | "n";

export function getPosition(): Promise<PositionSnapshot> {
  return invoke<PositionSnapshot>("get_position");
}

export function makeMove(
  from: string,
  to: string,
  promotion?: PromotionPiece,
): Promise<PositionSnapshot> {
  return invoke<PositionSnapshot>("make_move", {
    from,
    to,
    promotion: promotion ?? null,
  });
}

export function resetGame(): Promise<PositionSnapshot> {
  return invoke<PositionSnapshot>("reset_game");
}

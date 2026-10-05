# Architecture

## Layers

```text
React (Chessground + daisyUI)
        │  invoke
        ▼
Tauri commands (thin API)
        │
   ┌────┴────┐
   ▼         ▼
 game.rs   engine/   (stub in MVP)
(Shakmaty)  future UCI / install
```

- **Frontend** renders the board and chrome. It does not decide whether a move is legal.
- **`game`** owns the in-memory position, legal move map (`dests`), and game status.
- **`engine`** will own download/install/spawn/UCI later. MVP exposes stubs only so the boundary exists.

## Position contract

Commands return a snapshot roughly like:

```ts
{
  fen: string;
  turn: "white" | "black";
  dests: Record<string, string[]>; // e.g. { e2: ["e3","e4"] }
  lastMove: [string, string] | null;
  check: boolean;
  status: "playing" | "checkmate" | "stalemate";
  needsPromotion: boolean; // true if from→to requires Q/R/B/N choice
}
```

Flow:

1. UI loads snapshot via `get_position`.
2. Chessground gets `movable.dests` from that snapshot.
3. On move, UI calls `make_move(from, to, promotion?)`.
4. Backend validates with Shakmaty and returns a new snapshot (or an error).

## Why rules in Rust

Engine analysis, multi-engine UCI, and future features need a single authoritative position on the backend. Shakmaty matches the en-croissant-style split and avoids drifting from a JS rules engine.

## Styling zones

- **Tailwind / daisyUI:** layout, status panel, buttons, promotion dialog.
- **Chessground CSS:** squares, pieces, coordinates, animations inside the board container only.

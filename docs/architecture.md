# Architecture

## Layers

```text
React (Chessground + daisyUI tabs)
        │  invoke / events
        ▼
Tauri commands (thin API)
    │
    ├── game.rs        Shakmaty position / legal moves
    ├── engine/        catalog + download/install/remove (no UCI yet)
    ├── db/            catalog + download .db3 + browse games
    └── fs/            AppData paths + HTTP download helper
```

- **Frontend** renders the board and chrome. It does not decide whether a move is legal.
- **`game`** owns the in-memory position, legal move map (`dests`), and game status.
- **`engine`** manages UCI binaries on disk (catalog → AppData). Analysis/UCI spawn is later.
- **`db`** downloads ready game databases (`.db3`) and lists game metadata (no move replay yet).

## AppData layout (Linux)

`~/.local/share/com.mychessapp.app/`

```text
engines/
  engines.json
  stockfish-linux-x86-64/   # extracted binary
databases/
  databases.json
  caissabase-2024.db3
```

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

## Engines

- Built-in catalog (Linux Stockfish from GitHub releases).
- `install_engine` downloads archive, unpacks under `engines/{id}/`, `chmod +x`, updates `engines.json`.
- Progress via event `engine-download-progress`.
- No UCI process management in this phase.

## Databases

- Remote catalog: `https://www.encroissant.org/databases` (ready `.db3` files).
- `install_database` streams file into `databases/`, updates `databases.json`.
- Progress via event `db-download-progress`.
- `list_database_games` reads en-croissant SQLite schema (metadata only; moves BLOB not decoded).

## Why rules in Rust

Engine analysis, multi-engine UCI, and future features need a single authoritative position on the backend. Shakmaty matches the en-croissant-style split and avoids drifting from a JS rules engine.

## Styling zones

- **Tailwind / daisyUI:** layout, tabs, status panel, buttons, promotion dialog, tables.
- **Chessground CSS:** squares, pieces, coordinates, animations inside the board container only.

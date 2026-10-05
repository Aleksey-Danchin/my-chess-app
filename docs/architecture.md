# Architecture

## Layers

```text
React (Chessground + daisyUI tabs)
        │  invoke / events
        ▼
Tauri commands (thin API)
    │
    ├── game.rs              Shakmaty position / legal moves
    ├── engine/              install, profiles, UCI analysis + bestmove
    ├── db/                  download, Games browse, .mcidx search
    ├── repertoire/          DAG store + auto builder
    └── fs/                  AppData paths + HTTP download
```

## AppData (Linux)

`~/.local/share/com.mychessapp.app/`

```text
engines/engines.json, profiles.json, …
databases/*.db3, *.mcidx
repertoires/repertoires.json, {id}.json
```

## Position search

en-croissant-compatible Moves BLOB (`db/encoding.rs`). Sibling index `.mcidx` v2 beside `.db3`
(`build_database_index`): move blobs plus end-of-game material / pawn-home for reachability pruning.
`search_position` scans the index in parallel (with prune + mid-replay early exit), caches results
in memory by `(db, fen_key)`, and returns move stats (uci/san, W/D/L, percent). Rebuild the index
after upgrading from v1.

## Repertoire DAG

Nodes keyed by FEN without clocks. Edges: `manual` | `db` | `engine`. Opponent moves from DB filters; our replies = one engine bestmove. Transpositions share nodes. Builder: Stop / Continue / Rebuild.

## Analysis profiles

Named settings cloned onto the board session; Save / Save as.

## Styling

Tailwind/daisyUI for chrome; Chessground CSS only inside the board host.

# Roadmap

## Phase 0 — MVP board (done)

- Tauri window + Chessground board
- Legal moves via Shakmaty
- Status: turn, check, mate, stalemate
- New game + promotion UI

## Phase 1a — Install managers (current)

- Engine catalog → download/install/remove (Stockfish on Linux)
- Game DB catalog (encroissant.org) → download `.db3` + simple game browser
- No UCI analysis, no PGN import, no position index

## Phase 1b — Engine analysis

- Spawn UCI process
- Best moves / eval panel on the board tab
- Multi-engine support beyond Stockfish

## Phase 1c — Databases deep dive

- PGN → SQLite import
- Position index (`.ecsi`-style) + opening explorer
- Optional online explorer (Lichess / Masters)

## Phase 2 — Cross-platform

- Windows and macOS packaging
- Platform-specific engine binaries in the catalog

## Phase 3 — Play & history

- Play vs engine
- Move history / navigation
- Optional PGN export and local game storage

# my-chess-app

Desktop chess GUI: interactive board with legal moves, engine installer, and game-database browser. Built with Tauri 2, React, Chessground, and Shakmaty. Inspired by [en-croissant](https://github.com/franciscoBSalgueiro/en-croissant) architecture (not a fork).

## Stack

- **Shell:** Tauri 2
- **UI:** React + TypeScript + Vite + Tailwind CSS 4 + daisyUI 5
- **Board:** `@lichess-org/chessground`
- **Rules:** Shakmaty (Rust)
- **Engines / DBs:** Rust modules under `src-tauri` (download into AppData)
- **Package manager:** npm

## Prerequisites (Linux)

- Node.js + npm
- Rust (rustup)
- Tauri system dependencies — see [Tauri prerequisites](https://tauri.app/start/prerequisites/)

## Setup

```bash
npm install
```

## Development

```bash
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

## Docs

- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)

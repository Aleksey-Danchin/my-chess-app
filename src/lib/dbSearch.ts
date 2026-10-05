import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MoveStat = {
  uci: string;
  san: string;
  games: number;
  white: number;
  draw: number;
  black: number;
  percent: number;
};

export type PositionSearchResult = {
  fen: string;
  fenKey: string;
  totalGames: number;
  moves: MoveStat[];
};

export type IndexProgress = {
  dbId: string;
  done: number;
  total: number;
};

export function buildDatabaseIndex(dbId: string): Promise<void> {
  return invoke("build_database_index", { dbId });
}

export function databaseIndexStatus(dbId: string): Promise<boolean> {
  return invoke("database_index_status", { dbId });
}

export function searchDatabasePosition(
  dbId: string,
  fen: string,
): Promise<PositionSearchResult> {
  return invoke("search_database_position", { dbId, fen });
}

export function onDbIndexProgress(
  handler: (p: IndexProgress) => void,
): Promise<UnlistenFn> {
  return listen<IndexProgress>("db-index-progress", (e) => handler(e.payload));
}

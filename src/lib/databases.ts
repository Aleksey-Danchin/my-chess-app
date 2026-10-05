import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadProgress } from "./engines";

export type CatalogDatabase = {
  id: string;
  title: string;
  gameCount: number;
  storageSize: number;
  downloadLink: string;
};

export type DatabaseInfo = {
  id: string;
  title: string;
  path: string;
  gameCount: number;
  storageSize: number;
  installed: boolean;
};

export type GameRow = {
  id: number;
  white: string;
  black: string;
  result: string;
  date: string | null;
  event: string | null;
  whiteElo: number | null;
  blackElo: number | null;
};

export type GamesPage = {
  total: number;
  offset: number;
  limit: number;
  games: GameRow[];
};

export function listDatabaseCatalog(): Promise<CatalogDatabase[]> {
  return invoke("list_database_catalog");
}

export function listInstalledDatabases(): Promise<DatabaseInfo[]> {
  return invoke("list_installed_databases");
}

export function installDatabase(catalogId: string): Promise<DatabaseInfo> {
  return invoke("install_database", { catalogId });
}

export function removeDatabase(dbId: string): Promise<void> {
  return invoke("remove_database", { dbId });
}

export { cancelDownload } from "./engines";

export function listDatabaseGames(
  dbId: string,
  offset: number,
  limit: number,
): Promise<GamesPage> {
  return invoke("list_database_games", { dbId, offset, limit });
}

export function onDbDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("db-download-progress", (event) => {
    handler(event.payload);
  });
}

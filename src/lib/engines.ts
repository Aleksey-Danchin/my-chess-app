import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CatalogEngine = {
  id: string;
  name: string;
  version: string;
  downloadUrl: string;
  archivePath: string;
  downloadSize: number;
};

export type EngineInfo = {
  id: string;
  name: string;
  version: string;
  path: string;
  installed: boolean;
};

export type DownloadProgress = {
  id: string;
  downloaded: number;
  total: number | null;
};

export function listEngineCatalog(): Promise<CatalogEngine[]> {
  return invoke("list_engine_catalog");
}

export function listInstalledEngines(): Promise<EngineInfo[]> {
  return invoke("list_installed_engines");
}

export function installEngine(engineId: string): Promise<EngineInfo> {
  return invoke("install_engine", { engineId });
}

export function removeEngine(engineId: string): Promise<void> {
  return invoke("remove_engine", { engineId });
}

export function cancelDownload(downloadId: string): Promise<boolean> {
  return invoke("cancel_download", { downloadId });
}

export function onEngineDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("engine-download-progress", (event) => {
    handler(event.payload);
  });
}

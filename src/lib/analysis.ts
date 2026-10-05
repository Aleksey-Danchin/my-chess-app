import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type GoMode =
  | { kind: "infinite" }
  | { kind: "depth"; depth: number }
  | { kind: "movetime"; ms: number }
  | { kind: "nodes"; nodes: number };

export type AnalysisSettings = {
  go: GoMode;
  multipv: number;
  threads: number;
  hashMb: number;
};

export type AnalysisProfile = {
  id: string;
  name: string;
  engineId: string;
  settings: AnalysisSettings;
};

export type AnalysisLine = {
  multipv: number;
  depth: number;
  scoreCp: number | null;
  mate: number | null;
  nodes: number | null;
  nps: number | null;
  timeMs: number | null;
  pv: string[];
};

export type AnalysisUpdate = {
  fen: string;
  lines: AnalysisLine[];
  done: boolean;
};

export const defaultSettings = (): AnalysisSettings => ({
  go: { kind: "depth", depth: 20 },
  multipv: 3,
  threads: Math.max(
    1,
    Math.min(4, Math.floor((navigator.hardwareConcurrency || 4) / 2)),
  ),
  hashMb: 128,
});

export function maxEngineThreads(): Promise<number> {
  return invoke("max_engine_threads");
}

export function cloneSettings(settings: AnalysisSettings): AnalysisSettings {
  return structuredClone(settings);
}

export function listAnalysisProfiles(): Promise<AnalysisProfile[]> {
  return invoke("list_analysis_profiles");
}

export function createAnalysisProfile(input: {
  name: string;
  engineId: string;
  settings: AnalysisSettings;
}): Promise<AnalysisProfile> {
  return invoke("create_analysis_profile", { input });
}

export function updateAnalysisProfile(input: {
  id: string;
  name: string;
  engineId: string;
  settings: AnalysisSettings;
}): Promise<AnalysisProfile> {
  return invoke("update_analysis_profile", { input });
}

export function deleteAnalysisProfile(profileId: string): Promise<void> {
  return invoke("delete_analysis_profile", { profileId });
}

export function startAnalysis(
  engineId: string,
  fen: string,
  settings: AnalysisSettings,
): Promise<void> {
  return invoke("start_analysis", { engineId, fen, settings });
}

export function stopAnalysis(): Promise<void> {
  return invoke("stop_analysis");
}

export function onAnalysisUpdate(
  handler: (update: AnalysisUpdate) => void,
): Promise<UnlistenFn> {
  return listen<AnalysisUpdate>("analysis-update", (event) => {
    handler(event.payload);
  });
}

export function formatScore(line: AnalysisLine, turn: "white" | "black"): string {
  if (line.mate != null) {
    const mate = turn === "white" ? line.mate : -line.mate;
    return mate > 0 ? `M${mate}` : `M${mate}`;
  }
  if (line.scoreCp == null) {
    return "—";
  }
  const cp = turn === "white" ? line.scoreCp : -line.scoreCp;
  const pawns = (cp / 100).toFixed(2);
  return cp > 0 ? `+${pawns}` : pawns;
}

export function settingsEqual(a: AnalysisSettings, b: AnalysisSettings): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

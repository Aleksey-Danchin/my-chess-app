import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AnalysisLine } from "./analysis";

export type RepertoireFilters = {
  minGames: number;
  minPercent: number;
};

export type RepertoireNode = {
  fen: string;
  depthFromStart: number;
};

export type RepertoireEdge = {
  id: string;
  from: string;
  to: string;
  uci: string;
  san: string;
  source: string;
  games?: number | null;
  percent?: number | null;
  white?: number | null;
  draw?: number | null;
  black?: number | null;
};

export type Repertoire = {
  id: string;
  name: string;
  ourColor: string;
  setupUci: string[];
  startFen: string;
  dbId: string;
  profileId: string;
  filters: RepertoireFilters;
  maxDepth: number;
  nodes: Record<string, RepertoireNode>;
  edges: RepertoireEdge[];
  expanded: string[] | Set<string>;
  status: string;
  editorFen: string;
};

export type RepertoireSummary = {
  id: string;
  name: string;
  ourColor: string;
  status: string;
  nodeCount: number;
  edgeCount: number;
};

export type BuildProgress = {
  repertoireId: string;
  nodes: number;
  edges: number;
  queue: number;
  currentFen: string;
  status: string;
  phase: string;
  lastUci: string | null;
  lastSan: string | null;
  engineLines: AnalysisLine[];
  candidates: {
    uci: string;
    san: string;
    games?: number | null;
    percent?: number | null;
  }[];
  graphNodes: { key: string; depth: number }[];
  graphEdges: {
    from: string;
    to: string;
    san: string;
    source: string;
  }[];
  leafGames: number | null;
  scanDone: number | null;
  scanTotal: number | null;
};

export function listRepertoires(): Promise<RepertoireSummary[]> {
  return invoke("list_repertoires");
}

export function getRepertoire(id: string): Promise<Repertoire> {
  return invoke("get_repertoire", { id });
}

export function createRepertoire(input: {
  name: string;
  ourColor: string;
}): Promise<Repertoire> {
  return invoke("create_repertoire", { input });
}

export function updateRepertoire(input: Record<string, unknown>): Promise<Repertoire> {
  return invoke("update_repertoire", { input });
}

export function deleteRepertoire(id: string): Promise<void> {
  return invoke("delete_repertoire", { id });
}

export function startRepertoireBuild(repertoireId: string): Promise<Repertoire> {
  return invoke("start_repertoire_build", { repertoireId });
}

export function rebuildRepertoire(repertoireId: string): Promise<Repertoire> {
  return invoke("rebuild_repertoire", { repertoireId });
}

export function continueRepertoireBuild(repertoireId: string): Promise<Repertoire> {
  return invoke("continue_repertoire_build", { repertoireId });
}

export function stopRepertoireBuild(): Promise<void> {
  return invoke("stop_repertoire_build");
}

export function onRepertoireBuildProgress(
  handler: (p: BuildProgress) => void,
): Promise<UnlistenFn> {
  return listen<BuildProgress>("repertoire-build-progress", (e) =>
    handler(e.payload),
  );
}

/** FEN key without clocks — mirrors Rust fen_key. */
export function fenKeyFromFen(fen: string): string {
  const parts = fen.trim().split(/\s+/);
  if (parts.length >= 4) {
    return `${parts[0]} ${parts[1]} ${parts[2]} ${parts[3]}`;
  }
  return fen;
}

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChessBoard } from "./ChessBoard";
import { RepertoireGraph } from "./RepertoireGraph";
import {
  formatScore,
  listAnalysisProfiles,
  type AnalysisLine,
  type AnalysisProfile,
} from "../lib/analysis";
import { listInstalledDatabases, type DatabaseInfo } from "../lib/databases";
import type { PositionSnapshot } from "../lib/game";
import {
  continueRepertoireBuild,
  createRepertoire,
  deleteRepertoire,
  fenKeyFromFen,
  getRepertoire,
  listRepertoires,
  onRepertoireBuildProgress,
  rebuildRepertoire,
  startRepertoireBuild,
  stopRepertoireBuild,
  updateRepertoire,
  type BuildProgress,
  type Repertoire,
  type RepertoireSummary,
} from "../lib/repertoires";

function formatBuildError(raw: string): string {
  if (raw.startsWith("unsupported_index_version:")) {
    return "Индекс базы устарел (нужен v2). Откройте вкладку «Базы» и нажмите «Перестроить индекс» для MillionBase.";
  }
  if (raw === "index_missing") {
    return "Нет индекса поиска. Откройте вкладку «Базы» и нажмите «Построить индекс».";
  }
  return raw;
}

async function fenPosition(fen: string): Promise<PositionSnapshot> {
  return invoke("get_fen_position", { fen });
}

async function applyUci(fen: string, uci: string): Promise<PositionSnapshot> {
  return invoke("apply_uci_move", { fen, uci });
}

function withLastMove(
  snap: PositionSnapshot,
  uci: string | null | undefined,
): PositionSnapshot {
  if (!uci || uci.length < 4) {
    return { ...snap, lastMove: null };
  }
  return { ...snap, lastMove: [uci.slice(0, 2), uci.slice(2, 4)] };
}

function phaseLabel(phase: string): string {
  if (phase === "engine") return "ход движка";
  if (phase === "db") return "ходы из базы";
  if (phase === "db-scan") return "скан базы";
  return phase || "—";
}

export function RepertoiresPage() {
  const [list, setList] = useState<RepertoireSummary[]>([]);
  const [current, setCurrent] = useState<Repertoire | null>(null);
  const [position, setPosition] = useState<PositionSnapshot | null>(null);
  const [setupMode, setSetupMode] = useState(true);
  const [setupUci, setSetupUci] = useState<string[]>([]);
  const [databases, setDatabases] = useState<DatabaseInfo[]>([]);
  const [profiles, setProfiles] = useState<AnalysisProfile[]>([]);
  const [name, setName] = useState("Мой репертуар");
  const [ourColor, setOurColor] = useState<"white" | "black">("white");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [building, setBuilding] = useState(false);
  const [progress, setProgress] = useState<BuildProgress | null>(null);

  const refreshList = useCallback(async () => {
    setList(await listRepertoires());
  }, []);

  useEffect(() => {
    void Promise.all([
      refreshList(),
      listInstalledDatabases().then(setDatabases),
      listAnalysisProfiles().then(setProfiles),
    ]).catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, [refreshList]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onRepertoireBuildProgress((p) => {
      setProgress(p);
      if (p.currentFen) {
        void fenPosition(p.currentFen).then((snap) => {
          setPosition(withLastMove(snap, p.lastUci));
        });
      }
      if (
        p.status === "done" ||
        p.status === "stopped" ||
        p.status.startsWith("error:")
      ) {
        setBuilding(false);
        if (p.status.startsWith("error:")) {
          setError(formatBuildError(p.status.slice("error:".length)));
        }
        void getRepertoire(p.repertoireId).then((r) => {
          setCurrent(r);
          void fenPosition(r.editorFen || r.startFen).then(setPosition);
        });
        void refreshList();
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [refreshList]);

  const openRepertoire = async (id: string) => {
    setError(null);
    try {
      const r = await getRepertoire(id);
      setCurrent(r);
      setSetupUci(r.setupUci);
      setSetupMode(r.edges.length === 0 && r.nodes && Object.keys(r.nodes).length <= 1);
      setOurColor(r.ourColor === "black" ? "black" : "white");
      const snap = await fenPosition(r.editorFen || r.startFen);
      setPosition(snap);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCreate = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await createRepertoire({ name, ourColor });
      await refreshList();
      setCurrent(r);
      setSetupUci([]);
      setSetupMode(true);
      setPosition(await fenPosition(r.startFen));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (id: string) => {
    setBusy(true);
    try {
      await deleteRepertoire(id);
      if (current?.id === id) {
        setCurrent(null);
        setPosition(null);
      }
      await refreshList();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleUserMove = async (from: string, to: string) => {
    if (!position || !current || building) return;
    const uci = `${from}${to}`;
    try {
      const snap = await applyUci(position.fen, uci);
      if (snap.needsPromotion) {
        const withQ = await applyUci(position.fen, `${uci}q`);
        setPosition(withQ);
        if (setupMode) {
          const next = [...setupUci, `${uci}q`];
          setSetupUci(next);
          await updateRepertoire({
            id: current.id,
            setupUci: next,
            editorFen: withQ.fen,
          });
        }
        return;
      }
      setPosition(snap);
      if (setupMode) {
        const next = [...setupUci, uci];
        setSetupUci(next);
        const updated = await updateRepertoire({
          id: current.id,
          setupUci: next,
          editorFen: snap.fen,
        });
        setCurrent(updated);
      } else {
        const key = fenKeyFromFen(snap.fen);
        if (current.nodes[key]) {
          const updated = await updateRepertoire({
            id: current.id,
            editorFen: snap.fen,
          });
          setCurrent(updated);
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const markStart = async () => {
    if (!current || !position) return;
    setBusy(true);
    try {
      const updated = await updateRepertoire({
        id: current.id,
        startFen: position.fen,
        setupUci,
        ourColor,
        editorFen: position.fen,
      });
      setCurrent(updated);
      setSetupMode(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveMeta = async (patch: Record<string, unknown>) => {
    if (!current) return;
    try {
      const updated = await updateRepertoire({ id: current.id, ...patch });
      setCurrent(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const runBuild = async (mode: "start" | "continue" | "rebuild") => {
    if (!current) return;
    setBuilding(true);
    setError(null);
    setProgress(null);
    try {
      const fn =
        mode === "rebuild"
          ? rebuildRepertoire
          : mode === "continue"
            ? continueRepertoireBuild
            : startRepertoireBuild;
      // Returns immediately; work continues in background via progress events.
      const updated = await fn(current.id);
      setCurrent(updated);
    } catch (e) {
      setBuilding(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const currentKey = position ? fenKeyFromFen(position.fen) : null;
  const outgoing = useMemo(() => {
    if (!current || !currentKey) return [];
    return current.edges.filter((e) => e.from === currentKey);
  }, [current, currentKey]);

  const knownTargets = useMemo(() => {
    if (!current) return new Set<string>();
    return new Set(Object.keys(current.nodes));
  }, [current]);

  const graphNodes = useMemo(() => {
    if (building && progress?.graphNodes?.length) {
      return progress.graphNodes;
    }
    if (!current) return [];
    return Object.entries(current.nodes).map(([key, n]) => ({
      key,
      depth: n.depthFromStart,
    }));
  }, [building, progress, current]);

  const graphEdges = useMemo(() => {
    if (building && progress?.graphEdges?.length) {
      return progress.graphEdges;
    }
    if (!current) return [];
    return current.edges.map((e) => ({
      from: e.from,
      to: e.to,
      san: e.san,
      source: e.source,
    }));
  }, [building, progress, current]);

  const buildTurn: "white" | "black" = position?.turn ?? "white";
  const engineLines: AnalysisLine[] = progress?.engineLines ?? [];
  const candidates = progress?.candidates ?? [];

  const selectGraphKey = (key: string) => {
    if (building || !current) return;
    const node = current.nodes[key];
    if (node) {
      void fenPosition(node.fen).then(setPosition);
      void saveMeta({ editorFen: node.fen });
    }
  };

  return (
    <div className="space-y-6">
      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      <section className="rounded-box bg-base-100 space-y-3 p-4 shadow-sm">
        <h2 className="text-lg font-semibold">Репертуары</h2>
        <div className="flex flex-wrap items-end gap-2">
          <label className="form-control">
            <span className="label-text text-xs">Название</span>
            <input
              className="input input-bordered input-sm"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className="form-control">
            <span className="label-text text-xs">Цвет</span>
            <select
              className="select select-bordered select-sm"
              value={ourColor}
              onChange={(e) => setOurColor(e.target.value as "white" | "black")}
            >
              <option value="white">Белые</option>
              <option value="black">Чёрные</option>
            </select>
          </label>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={busy}
            onClick={() => void handleCreate()}
          >
            Создать
          </button>
        </div>
        <div className="space-y-2">
          {list.length === 0 ? (
            <p className="text-sm opacity-70">Пока нет репертуаров.</p>
          ) : (
            list.map((r) => (
              <div
                key={r.id}
                className="flex flex-wrap items-center justify-between gap-2 border-t border-base-200 pt-2"
              >
                <button
                  type="button"
                  className="text-left"
                  onClick={() => void openRepertoire(r.id)}
                >
                  <span className="font-medium">{r.name}</span>
                  <span className="ml-2 text-xs opacity-70">
                    {r.ourColor} · {r.nodeCount} узлов · {r.status}
                  </span>
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-xs text-error"
                  onClick={() => void handleDelete(r.id)}
                >
                  Удалить
                </button>
              </div>
            ))
          )}
        </div>
      </section>

      {current && position ? (
        <section className="space-y-4">
          <div className="rounded-box bg-base-100 space-y-3 p-4 shadow-sm">
            <h3 className="font-semibold">{current.name}</h3>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="form-control">
                <span className="label-text text-xs">База партий</span>
                <select
                  className="select select-bordered select-sm"
                  value={current.dbId}
                  onChange={(e) => void saveMeta({ dbId: e.target.value })}
                >
                  <option value="">Выберите базу</option>
                  {databases.map((d) => (
                    <option key={d.id} value={d.id} disabled={!d.indexed}>
                      {d.title}
                      {d.indexed ? "" : " (нужен индекс)"}
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-control">
                <span className="label-text text-xs">Профиль движка</span>
                <select
                  className="select select-bordered select-sm"
                  value={current.profileId}
                  onChange={(e) => void saveMeta({ profileId: e.target.value })}
                >
                  <option value="">Выберите профиль</option>
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-control">
                <span className="label-text text-xs">Мин. партий</span>
                <input
                  type="number"
                  className="input input-bordered input-sm"
                  min={1}
                  value={current.filters.minGames}
                  onChange={(e) =>
                    void saveMeta({
                      filters: {
                        ...current.filters,
                        minGames: Number(e.target.value) || 1,
                      },
                    })
                  }
                />
              </label>
              <label className="form-control">
                <span className="label-text text-xs">Мин. %</span>
                <input
                  type="number"
                  className="input input-bordered input-sm"
                  min={0}
                  step={0.5}
                  value={current.filters.minPercent}
                  onChange={(e) =>
                    void saveMeta({
                      filters: {
                        ...current.filters,
                        minPercent: Number(e.target.value) || 0,
                      },
                    })
                  }
                />
              </label>
              <label className="form-control">
                <span className="label-text text-xs">Макс. глубина (0 = ∞)</span>
                <input
                  type="number"
                  className="input input-bordered input-sm"
                  min={0}
                  value={current.maxDepth}
                  onChange={(e) =>
                    void saveMeta({ maxDepth: Number(e.target.value) || 0 })
                  }
                />
              </label>
            </div>

            <div className="flex flex-wrap gap-2">
              {setupMode ? (
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  disabled={busy}
                  onClick={() => void markStart()}
                >
                  Старт репертуара здесь
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    className="btn btn-primary btn-sm"
                    disabled={building || !current.dbId || !current.profileId}
                    onClick={() => void runBuild("start")}
                  >
                    Собрать
                  </button>
                  <button
                    type="button"
                    className="btn btn-outline btn-sm"
                    disabled={building || !current.dbId || !current.profileId}
                    onClick={() => void runBuild("continue")}
                  >
                    Продолжить
                  </button>
                  <button
                    type="button"
                    className="btn btn-outline btn-sm"
                    disabled={building || !current.dbId || !current.profileId}
                    onClick={() => void runBuild("rebuild")}
                  >
                    Пересоздать
                  </button>
                  {building ? (
                    <button
                      type="button"
                      className="btn btn-warning btn-sm"
                      onClick={() => void stopRepertoireBuild()}
                    >
                      Стоп
                    </button>
                  ) : null}
                </>
              )}
            </div>

            {progress ? (
              <p className="text-sm opacity-70">
                Сборка: узлов {progress.nodes}, рёбер {progress.edges}, очередь{" "}
                {progress.queue} · {progress.status}
                {progress.phase ? ` · ${phaseLabel(progress.phase)}` : ""}
                {progress.lastSan ? ` · ход ${progress.lastSan}` : ""}
                {progress.leafGames != null
                  ? ` · частота ${progress.leafGames.toLocaleString("ru-RU")}`
                  : ""}
                {progress.scanDone != null && progress.scanTotal != null
                  ? ` · скан ${progress.scanDone.toLocaleString("ru-RU")} / ${progress.scanTotal.toLocaleString("ru-RU")}`
                  : ""}
              </p>
            ) : null}
            {setupMode ? (
              <p className="text-sm opacity-70">
                Сделайте ходы до стартовой позиции, затем нажмите «Старт репертуара
                здесь». Цвет репертуара: {ourColor === "white" ? "белые" : "чёрные"}.
              </p>
            ) : null}
          </div>

          <div className="flex flex-col gap-4 lg:flex-row lg:items-start">
            <div className="flex w-full max-w-[min(72vmin,32rem)] shrink-0 flex-col gap-3">
              <div className="w-full">
                <ChessBoard
                  position={position}
                  interactive={!building && setupMode}
                  onUserMove={(from, to) => void handleUserMove(from, to)}
                />
              </div>
              {building && progress ? (
                <div className="rounded-box bg-base-100 max-w-md space-y-2 p-3 text-sm shadow-sm">
                  <p className="font-medium">
                    Сейчас: {phaseLabel(progress.phase)}
                    {progress.leafGames != null ? (
                      <span className="opacity-70">
                        {" "}
                        (частота {progress.leafGames.toLocaleString("ru-RU")})
                      </span>
                    ) : null}
                    {progress.lastSan ? (
                      <span className="text-primary"> → {progress.lastSan}</span>
                    ) : null}
                  </p>
                  {progress.phase === "db-scan" &&
                  progress.scanDone != null &&
                  progress.scanTotal != null ? (
                    <div>
                      <p className="opacity-70">
                        Сканирование индекса MillionBase…
                      </p>
                      <progress
                        className="progress progress-primary mt-1 w-full"
                        value={progress.scanDone}
                        max={Math.max(1, progress.scanTotal)}
                      />
                    </div>
                  ) : null}
                  {progress.phase === "engine" && engineLines.length > 0 ? (
                    <div className="overflow-x-auto">
                      <table className="table table-xs">
                        <thead>
                          <tr>
                            <th>#</th>
                            <th>Оценка</th>
                            <th>Глуб.</th>
                            <th>PV</th>
                          </tr>
                        </thead>
                        <tbody>
                          {engineLines.map((line) => (
                            <tr key={line.multipv}>
                              <td>{line.multipv}</td>
                              <td className="font-mono">
                                {formatScore(line, buildTurn)}
                              </td>
                              <td>{line.depth}</td>
                              <td className="font-mono whitespace-nowrap">
                                {line.pv.slice(0, 8).join(" ")}
                                {line.pv.length > 8 ? "…" : ""}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : null}
                  {progress.phase === "db" && candidates.length > 0 ? (
                    <ul className="space-y-1">
                      {candidates.map((c) => (
                        <li
                          key={c.uci}
                          className={
                            c.uci === progress.lastUci
                              ? "text-primary font-medium"
                              : ""
                          }
                        >
                          <span className="font-mono">{c.san}</span>
                          {c.games != null ? (
                            <span className="opacity-60">
                              {" "}
                              · {c.games} / {c.percent?.toFixed(1)}%
                            </span>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              ) : null}
            </div>
            <div className="min-w-0 flex-1 space-y-4">
              <div className="space-y-2">
                <h4 className="font-medium">Граф решений</h4>
                <RepertoireGraph
                  nodes={graphNodes}
                  edges={graphEdges}
                  currentKey={currentKey}
                  onSelectKey={building ? undefined : selectGraphKey}
                />
                <p className="text-xs opacity-60">
                  Синие рёбра — ход движка, вторичные — ответы из базы.
                </p>
              </div>
              <div className="space-y-2">
                <h4 className="font-medium">Ходы из позиции</h4>
                {outgoing.length === 0 ? (
                  <p className="text-sm opacity-70">
                    {setupMode
                      ? "Пока setup — ходите по доске."
                      : "Нет исходящих рёбер (соберите репертуар или выберите другую позицию)."}
                  </p>
                ) : (
                  <ul className="space-y-1">
                    {outgoing.map((e) => {
                      const transposition =
                        knownTargets.has(e.to) &&
                        current.edges.some(
                          (o) => o.to === e.to && o.id !== e.id,
                        );
                      return (
                        <li key={e.id}>
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm justify-start"
                            disabled={building}
                            onClick={() => {
                              const node = current.nodes[e.to];
                              if (node) {
                                void fenPosition(node.fen).then(setPosition);
                                void saveMeta({ editorFen: node.fen });
                              }
                            }}
                          >
                            <span className="font-mono">{e.san}</span>
                            <span className="opacity-60"> ({e.source})</span>
                            {e.games != null ? (
                              <span className="opacity-60">
                                {" "}
                                · {e.games} / {e.percent?.toFixed(1)}%
                              </span>
                            ) : null}
                            {transposition ? (
                              <span className="badge badge-ghost badge-sm ml-1">
                                транспозиция
                              </span>
                            ) : null}
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                )}
                <p className="break-all text-xs opacity-50">{position.fen}</p>
              </div>
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}

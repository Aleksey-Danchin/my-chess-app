import { useEffect, useMemo, useState } from "react";
import {
  cancelDownload,
  installDatabase,
  listDatabaseCatalog,
  listDatabaseGames,
  listInstalledDatabases,
  onDbDownloadProgress,
  removeDatabase,
  type CatalogDatabase,
  type DatabaseInfo,
  type GameRow,
} from "../lib/databases";
import { isDownloadCancelled } from "../lib/download";
import { formatBytes, formatProgress } from "../lib/format";

const PAGE_SIZE = 50;

type ProgressMap = Record<string, { downloaded: number; total: number | null }>;

export function DatabasesPage() {
  const [catalog, setCatalog] = useState<CatalogDatabase[]>([]);
  const [installed, setInstalled] = useState<DatabaseInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [games, setGames] = useState<GameRow[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [progress, setProgress] = useState<ProgressMap>({});
  const [busyId, setBusyId] = useState<string | null>(null);
  const [loadingGames, setLoadingGames] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const installedIds = useMemo(
    () => new Set(installed.map((d) => d.id)),
    [installed],
  );

  const refresh = async () => {
    const [cat, inst] = await Promise.all([
      listDatabaseCatalog(),
      listInstalledDatabases(),
    ]);
    setCatalog(cat);
    setInstalled(inst);
    if (selectedId && !inst.some((d) => d.id === selectedId)) {
      setSelectedId(null);
      setGames([]);
      setTotal(0);
    }
  };

  useEffect(() => {
    void refresh().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onDbDownloadProgress((p) => {
      setProgress((prev) => ({
        ...prev,
        [p.id]: { downloaded: p.downloaded, total: p.total },
      }));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const clearProgress = (id: string) => {
    setProgress((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  };

  const loadGames = async (dbId: string, nextOffset: number) => {
    setLoadingGames(true);
    setError(null);
    try {
      const page = await listDatabaseGames(dbId, nextOffset, PAGE_SIZE);
      setGames(page.games);
      setTotal(page.total);
      setOffset(page.offset);
      setSelectedId(dbId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingGames(false);
    }
  };

  const handleInstall = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await installDatabase(id);
      await refresh();
      clearProgress(id);
    } catch (e) {
      clearProgress(id);
      if (!isDownloadCancelled(e)) {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusyId(null);
    }
  };

  const handleCancel = async (id: string) => {
    try {
      await cancelDownload(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRemove = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await removeDatabase(id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  const pageEnd = Math.min(offset + games.length, total);

  return (
    <div className="space-y-6">
      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Каталог</h2>
        <p className="text-sm opacity-70">
          Готовые базы с encroissant.org. Файлы могут быть большими (1+ GB).
        </p>
        <div className="space-y-3">
          {catalog.map((db) => {
            const isInstalled = installedIds.has(db.id);
            const p = progress[db.id];
            const busy = busyId === db.id;
            return (
              <div key={db.id} className="rounded-box bg-base-100 p-4 shadow-sm">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="font-semibold">{db.title}</p>
                    <p className="text-sm opacity-70">
                      {db.gameCount.toLocaleString("ru-RU")} партий ·{" "}
                      {formatBytes(db.storageSize)}
                    </p>
                    {p ? (
                      <p className="mt-1 text-sm">
                        {formatProgress(p.downloaded, p.total)}
                      </p>
                    ) : null}
                    {p && p.total ? (
                      <progress
                        className="progress progress-primary mt-2 w-full max-w-md"
                        value={p.downloaded}
                        max={p.total}
                      />
                    ) : null}
                  </div>
                  <div className="flex gap-2">
                    {busy ? (
                      <button
                        type="button"
                        className="btn btn-outline btn-sm"
                        onClick={() => void handleCancel(db.id)}
                      >
                        Отменить
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="btn btn-primary btn-sm"
                        disabled={isInstalled}
                        onClick={() => void handleInstall(db.id)}
                      >
                        {isInstalled ? "Установлена" : "Скачать"}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </section>

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Установленные</h2>
        {installed.length === 0 ? (
          <p className="text-sm opacity-70">Базы ещё не установлены.</p>
        ) : (
          <div className="space-y-3">
            {installed.map((db) => (
              <div
                key={db.id}
                className="rounded-box bg-base-100 p-4 shadow-sm"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="font-semibold">{db.title}</p>
                    <p className="text-sm opacity-70">
                      {db.gameCount.toLocaleString("ru-RU")} партий
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <button
                      type="button"
                      className="btn btn-outline btn-sm"
                      onClick={() => void loadGames(db.id, 0)}
                    >
                      Открыть
                    </button>
                    <button
                      type="button"
                      className="btn btn-outline btn-error btn-sm"
                      disabled={busyId === db.id}
                      onClick={() => void handleRemove(db.id)}
                    >
                      Удалить
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {selectedId ? (
        <section className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-lg font-semibold">Партии</h2>
            <p className="text-sm opacity-70">
              {total === 0
                ? "Нет партий"
                : `${offset + 1}–${pageEnd} из ${total.toLocaleString("ru-RU")}`}
            </p>
          </div>

          {loadingGames ? (
            <span className="loading loading-spinner loading-md" />
          ) : (
            <div className="overflow-x-auto rounded-box bg-base-100 shadow-sm">
              <table className="table table-zebra table-sm">
                <thead>
                  <tr>
                    <th>Белые</th>
                    <th>Чёрные</th>
                    <th>Результат</th>
                    <th>Дата</th>
                    <th>Турнир</th>
                  </tr>
                </thead>
                <tbody>
                  {games.map((g) => (
                    <tr key={g.id}>
                      <td>
                        {g.white}
                        {g.whiteElo != null ? (
                          <span className="opacity-60"> ({g.whiteElo})</span>
                        ) : null}
                      </td>
                      <td>
                        {g.black}
                        {g.blackElo != null ? (
                          <span className="opacity-60"> ({g.blackElo})</span>
                        ) : null}
                      </td>
                      <td>{g.result}</td>
                      <td>{g.date ?? "—"}</td>
                      <td>{g.event ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          <div className="flex gap-2">
            <button
              type="button"
              className="btn btn-sm"
              disabled={offset === 0 || loadingGames}
              onClick={() =>
                selectedId &&
                void loadGames(selectedId, Math.max(0, offset - PAGE_SIZE))
              }
            >
              Назад
            </button>
            <button
              type="button"
              className="btn btn-sm"
              disabled={offset + PAGE_SIZE >= total || loadingGames}
              onClick={() =>
                selectedId && void loadGames(selectedId, offset + PAGE_SIZE)
              }
            >
              Вперёд
            </button>
          </div>
        </section>
      ) : null}
    </div>
  );
}

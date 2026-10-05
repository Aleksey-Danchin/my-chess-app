import { useEffect, useMemo, useState } from "react";
import { isDownloadCancelled } from "../lib/download";
import {
  cancelDownload,
  installEngine,
  listEngineCatalog,
  listInstalledEngines,
  onEngineDownloadProgress,
  removeEngine,
  type CatalogEngine,
  type EngineInfo,
} from "../lib/engines";
import { formatBytes, formatProgress } from "../lib/format";

type ProgressMap = Record<string, { downloaded: number; total: number | null }>;

export function EnginesPage() {
  const [catalog, setCatalog] = useState<CatalogEngine[]>([]);
  const [installed, setInstalled] = useState<EngineInfo[]>([]);
  const [progress, setProgress] = useState<ProgressMap>({});
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const installedIds = useMemo(
    () => new Set(installed.map((e) => e.id)),
    [installed],
  );

  const refresh = async () => {
    const [cat, inst] = await Promise.all([
      listEngineCatalog(),
      listInstalledEngines(),
    ]);
    setCatalog(cat);
    setInstalled(inst);
  };

  useEffect(() => {
    void refresh().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onEngineDownloadProgress((p) => {
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

  const handleInstall = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await installEngine(id);
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
      await removeEngine(id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="space-y-6">
      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Каталог</h2>
        {catalog.length === 0 ? (
          <p className="text-sm opacity-70">
            Для этой платформы в каталоге пока нет движков.
          </p>
        ) : (
          <div className="space-y-3">
            {catalog.map((engine) => {
              const isInstalled = installedIds.has(engine.id);
              const p = progress[engine.id];
              const busy = busyId === engine.id;
              return (
                <div
                  key={engine.id}
                  className="rounded-box bg-base-100 p-4 shadow-sm"
                >
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                      <p className="font-semibold">{engine.name}</p>
                      <p className="text-sm opacity-70">
                        {engine.version} · ~{formatBytes(engine.downloadSize)}
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
                          onClick={() => void handleCancel(engine.id)}
                        >
                          Отменить
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="btn btn-primary btn-sm"
                          disabled={isInstalled}
                          onClick={() => void handleInstall(engine.id)}
                        >
                          {isInstalled ? "Установлен" : "Скачать"}
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      <section className="space-y-3">
        <h2 className="text-lg font-semibold">Установленные</h2>
        {installed.length === 0 ? (
          <p className="text-sm opacity-70">Движки ещё не установлены.</p>
        ) : (
          <div className="space-y-3">
            {installed.map((engine) => (
              <div
                key={engine.id}
                className="rounded-box bg-base-100 p-4 shadow-sm"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="font-semibold">{engine.name}</p>
                    <p className="text-sm opacity-70">{engine.version}</p>
                    <p className="mt-1 break-all text-xs opacity-60">
                      {engine.path}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="btn btn-outline btn-error btn-sm"
                    disabled={busyId === engine.id}
                    onClick={() => void handleRemove(engine.id)}
                  >
                    Удалить
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

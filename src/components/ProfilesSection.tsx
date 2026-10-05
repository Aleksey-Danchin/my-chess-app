import { useEffect, useState } from "react";
import type { EngineInfo } from "../lib/engines";
import { listInstalledEngines } from "../lib/engines";
import {
  cloneSettings,
  createAnalysisProfile,
  defaultSettings,
  deleteAnalysisProfile,
  listAnalysisProfiles,
  maxEngineThreads,
  updateAnalysisProfile,
  type AnalysisProfile,
  type AnalysisSettings,
  type GoMode,
} from "../lib/analysis";

type Props = {
  /** Called after profiles change so Board can refresh the list. */
  onProfilesChanged?: () => void;
};

function GoModeFields({
  go,
  onChange,
}: {
  go: GoMode;
  onChange: (go: GoMode) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      <select
        className="select select-bordered select-sm"
        value={go.kind}
        onChange={(e) => {
          const kind = e.target.value as GoMode["kind"];
          if (kind === "infinite") onChange({ kind });
          else if (kind === "depth") onChange({ kind, depth: 20 });
          else if (kind === "movetime") onChange({ kind, ms: 3000 });
          else onChange({ kind, nodes: 1_000_000 });
        }}
      >
        <option value="depth">Глубина</option>
        <option value="movetime">Время (мс)</option>
        <option value="nodes">Узлы</option>
        <option value="infinite">Бесконечно</option>
      </select>
      {go.kind === "depth" ? (
        <input
          type="number"
          className="input input-bordered input-sm w-24"
          min={1}
          max={99}
          value={go.depth}
          onChange={(e) =>
            onChange({ kind: "depth", depth: Number(e.target.value) || 1 })
          }
        />
      ) : null}
      {go.kind === "movetime" ? (
        <input
          type="number"
          className="input input-bordered input-sm w-28"
          min={100}
          step={100}
          value={go.ms}
          onChange={(e) =>
            onChange({ kind: "movetime", ms: Number(e.target.value) || 100 })
          }
        />
      ) : null}
      {go.kind === "nodes" ? (
        <input
          type="number"
          className="input input-bordered input-sm w-32"
          min={1000}
          step={1000}
          value={go.nodes}
          onChange={(e) =>
            onChange({ kind: "nodes", nodes: Number(e.target.value) || 1000 })
          }
        />
      ) : null}
    </div>
  );
}

export function SettingsFields({
  engines,
  engineId,
  settings,
  maxThreads = 128,
  onEngineIdChange,
  onSettingsChange,
}: {
  engines: EngineInfo[];
  engineId: string;
  settings: AnalysisSettings;
  maxThreads?: number;
  onEngineIdChange: (id: string) => void;
  onSettingsChange: (settings: AnalysisSettings) => void;
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <label className="form-control">
        <span className="label-text text-xs">Движок</span>
        <select
          className="select select-bordered select-sm"
          value={engineId}
          onChange={(e) => onEngineIdChange(e.target.value)}
        >
          <option value="" disabled>
            Выберите движок
          </option>
          {engines.map((e) => (
            <option key={e.id} value={e.id}>
              {e.name} ({e.version})
            </option>
          ))}
        </select>
      </label>
      <label className="form-control">
        <span className="label-text text-xs">Режим поиска</span>
        <GoModeFields
          go={settings.go}
          onChange={(go) => onSettingsChange({ ...settings, go })}
        />
      </label>
      <label className="form-control">
        <span className="label-text text-xs">MultiPV</span>
        <input
          type="number"
          className="input input-bordered input-sm"
          min={1}
          max={20}
          value={settings.multipv}
          onChange={(e) =>
            onSettingsChange({
              ...settings,
              multipv: Number(e.target.value) || 1,
            })
          }
        />
      </label>
      <label className="form-control">
        <span className="label-text text-xs">
          Threads (макс. {maxThreads} = ½ CPU)
        </span>
        <input
          type="number"
          className="input input-bordered input-sm"
          min={1}
          max={maxThreads}
          value={settings.threads}
          onChange={(e) =>
            onSettingsChange({
              ...settings,
              threads: Math.min(
                maxThreads,
                Math.max(1, Number(e.target.value) || 1),
              ),
            })
          }
        />
      </label>
      <label className="form-control">
        <span className="label-text text-xs">Hash (MB)</span>
        <input
          type="number"
          className="input input-bordered input-sm"
          min={16}
          max={8192}
          step={16}
          value={settings.hashMb}
          onChange={(e) =>
            onSettingsChange({
              ...settings,
              hashMb: Number(e.target.value) || 16,
            })
          }
        />
      </label>
    </div>
  );
}

export function ProfilesSection({ onProfilesChanged }: Props) {
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [profiles, setProfiles] = useState<AnalysisProfile[]>([]);
  const [name, setName] = useState("Быстрый анализ");
  const [engineId, setEngineId] = useState("");
  const [settings, setSettings] = useState<AnalysisSettings>(defaultSettings);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [maxThreads, setMaxThreads] = useState(4);

  const refresh = async () => {
    const [eng, prof, maxT] = await Promise.all([
      listInstalledEngines(),
      listAnalysisProfiles(),
      maxEngineThreads(),
    ]);
    setEngines(eng);
    setProfiles(prof);
    setMaxThreads(maxT);
    if (!engineId && eng[0]) {
      setEngineId(eng[0].id);
    }
  };

  useEffect(() => {
    void refresh().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, []);

  const resetForm = () => {
    setEditingId(null);
    setName("Быстрый анализ");
    setSettings(defaultSettings());
    if (engines[0]) setEngineId(engines[0].id);
  };

  const handleSave = async () => {
    if (!engineId) {
      setError("Сначала установите и выберите движок");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (editingId) {
        await updateAnalysisProfile({
          id: editingId,
          name,
          engineId,
          settings,
        });
      } else {
        await createAnalysisProfile({ name, engineId, settings });
      }
      await refresh();
      onProfilesChanged?.();
      resetForm();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleEdit = (profile: AnalysisProfile) => {
    setEditingId(profile.id);
    setName(profile.name);
    setEngineId(profile.engineId);
    setSettings(cloneSettings(profile.settings));
  };

  const handleDelete = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await deleteAnalysisProfile(id);
      await refresh();
      onProfilesChanged?.();
      if (editingId === id) resetForm();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-3">
      <h2 className="text-lg font-semibold">Профили анализа</h2>
      <p className="text-sm opacity-70">
        Именованные настройки движка: режим поиска, MultiPV, Threads, Hash.
      </p>

      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      {engines.length === 0 ? (
        <p className="text-sm opacity-70">
          Установите движок выше, затем создайте профиль.
        </p>
      ) : (
        <div className="rounded-box bg-base-100 space-y-3 p-4 shadow-sm">
          <label className="form-control">
            <span className="label-text text-xs">Название</span>
            <input
              className="input input-bordered input-sm"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <SettingsFields
            engines={engines}
            engineId={engineId}
            settings={settings}
            maxThreads={maxThreads}
            onEngineIdChange={setEngineId}
            onSettingsChange={setSettings}
          />
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={busy}
              onClick={() => void handleSave()}
            >
              {editingId ? "Сохранить профиль" : "Создать профиль"}
            </button>
            {editingId ? (
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={resetForm}
              >
                Отмена
              </button>
            ) : null}
          </div>
        </div>
      )}

      <div className="space-y-2">
        {profiles.length === 0 ? (
          <p className="text-sm opacity-70">Профилей пока нет.</p>
        ) : (
          profiles.map((p) => (
            <div
              key={p.id}
              className="rounded-box bg-base-100 flex flex-wrap items-center justify-between gap-3 p-3 shadow-sm"
            >
              <div>
                <p className="font-medium">{p.name}</p>
                <p className="text-xs opacity-70">
                  {p.engineId} · MultiPV {p.settings.multipv} ·{" "}
                  {p.settings.go.kind}
                </p>
              </div>
              <div className="flex gap-2">
                <button
                  type="button"
                  className="btn btn-outline btn-sm"
                  onClick={() => handleEdit(p)}
                >
                  Изменить
                </button>
                <button
                  type="button"
                  className="btn btn-outline btn-error btn-sm"
                  disabled={busy}
                  onClick={() => void handleDelete(p.id)}
                >
                  Удалить
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

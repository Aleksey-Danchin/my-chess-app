import { useEffect, useMemo, useState } from "react";
import {
  cloneSettings,
  createAnalysisProfile,
  formatScore,
  listAnalysisProfiles,
  maxEngineThreads,
  onAnalysisUpdate,
  settingsEqual,
  startAnalysis,
  stopAnalysis,
  updateAnalysisProfile,
  type AnalysisLine,
  type AnalysisProfile,
  type AnalysisSettings,
} from "../lib/analysis";
import { listInstalledEngines, type EngineInfo } from "../lib/engines";
import { SettingsFields } from "./ProfilesSection";

type Props = {
  fen: string | null;
  turn: "white" | "black";
};

type BoardSession = {
  profileId: string | null;
  engineId: string;
  settings: AnalysisSettings;
};

export function AnalysisPanel({ fen, turn }: Props) {
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [profiles, setProfiles] = useState<AnalysisProfile[]>([]);
  const [session, setSession] = useState<BoardSession | null>(null);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const [lines, setLines] = useState<AnalysisLine[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saveAsName, setSaveAsName] = useState("");
  const [showSaveAs, setShowSaveAs] = useState(false);
  const [busySave, setBusySave] = useState(false);
  const [maxThreads, setMaxThreads] = useState(4);

  const refreshMeta = async () => {
    const [eng, prof, maxT] = await Promise.all([
      listInstalledEngines(),
      listAnalysisProfiles(),
      maxEngineThreads(),
    ]);
    setEngines(eng);
    setProfiles(prof);
    setMaxThreads(maxT);
    return { eng, prof };
  };

  useEffect(() => {
    void refreshMeta().catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
    return () => {
      void stopAnalysis();
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onAnalysisUpdate((update) => {
      if (fen && update.fen !== fen) {
        return;
      }
      setLines(update.lines);
      setDone(update.done);
      if (update.done) {
        setRunning(false);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [fen]);

  // Restart analysis when FEN changes while running.
  useEffect(() => {
    if (!running || !session || !fen) {
      return;
    }
    setLines([]);
    setDone(false);
    void startAnalysis(session.engineId, fen, session.settings).catch((e) => {
      setError(e instanceof Error ? e.message : String(e));
      setRunning(false);
    });
  }, [fen]);

  const selectedProfile = useMemo(
    () => profiles.find((p) => p.id === session?.profileId) ?? null,
    [profiles, session?.profileId],
  );

  const dirty = useMemo(() => {
    if (!session || !selectedProfile) {
      return false;
    }
    return (
      session.engineId !== selectedProfile.engineId ||
      !settingsEqual(session.settings, selectedProfile.settings)
    );
  }, [session, selectedProfile]);

  const applyProfile = (profileId: string) => {
    const profile = profiles.find((p) => p.id === profileId);
    if (!profile) {
      return;
    }
    setSession({
      profileId: profile.id,
      engineId: profile.engineId,
      settings: cloneSettings(profile.settings),
    });
    setLines([]);
    setDone(false);
    setShowSaveAs(false);
    setError(null);
  };

  const handleStart = async () => {
    if (!session || !fen) {
      setError("Выберите профиль и дождитесь позиции");
      return;
    }
    if (!session.engineId) {
      setError("В сессии не выбран движок");
      return;
    }
    setError(null);
    setLines([]);
    setDone(false);
    setRunning(true);
    try {
      await startAnalysis(session.engineId, fen, session.settings);
    } catch (e) {
      setRunning(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleStop = async () => {
    try {
      await stopAnalysis();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };

  const handleSaveProfile = async () => {
    if (!session?.profileId || !selectedProfile) {
      return;
    }
    setBusySave(true);
    setError(null);
    try {
      await updateAnalysisProfile({
        id: selectedProfile.id,
        name: selectedProfile.name,
        engineId: session.engineId,
        settings: session.settings,
      });
      const { prof } = await refreshMeta();
      setProfiles(prof);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusySave(false);
    }
  };

  const handleSaveAs = async () => {
    if (!session) {
      return;
    }
    const name = saveAsName.trim();
    if (!name) {
      setError("Укажите имя нового профиля");
      return;
    }
    setBusySave(true);
    setError(null);
    try {
      const created = await createAnalysisProfile({
        name,
        engineId: session.engineId,
        settings: session.settings,
      });
      const { prof } = await refreshMeta();
      setProfiles(prof);
      setSession({
        profileId: created.id,
        engineId: created.engineId,
        settings: cloneSettings(created.settings),
      });
      setShowSaveAs(false);
      setSaveAsName("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusySave(false);
    }
  };

  if (engines.length === 0) {
    return (
      <section className="rounded-box bg-base-100 p-4 shadow-sm">
        <h2 className="font-semibold">Анализ</h2>
        <p className="mt-1 text-sm opacity-70">
          Установите движок на вкладке «Движки», затем создайте профиль.
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-box bg-base-100 space-y-4 p-4 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-semibold">Анализ позиции</h2>
        {running ? (
          <span className="badge badge-info badge-sm">считает…</span>
        ) : done ? (
          <span className="badge badge-success badge-sm">готово</span>
        ) : null}
      </div>

      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      <label className="form-control max-w-md">
        <span className="label-text text-xs">Профиль (клонируется на доску)</span>
        <select
          className="select select-bordered select-sm"
          value={session?.profileId ?? ""}
          onChange={(e) => {
            if (e.target.value) applyProfile(e.target.value);
          }}
        >
          <option value="" disabled>
            {profiles.length ? "Выберите профиль" : "Нет профилей — создайте на вкладке Движки"}
          </option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      {session ? (
        <>
          <SettingsFields
            engines={engines}
            engineId={session.engineId}
            settings={session.settings}
            maxThreads={maxThreads}
            onEngineIdChange={(engineId) =>
              setSession((s) => (s ? { ...s, engineId } : s))
            }
            onSettingsChange={(settings) =>
              setSession((s) => (s ? { ...s, settings } : s))
            }
          />

          <div className="flex flex-wrap gap-2">
            {running ? (
              <button
                type="button"
                className="btn btn-warning btn-sm"
                onClick={() => void handleStop()}
              >
                Стоп
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-primary btn-sm"
                disabled={!fen}
                onClick={() => void handleStart()}
              >
                Оценить позицию
              </button>
            )}
            <button
              type="button"
              className="btn btn-outline btn-sm"
              disabled={!session.profileId || !dirty || busySave || running}
              onClick={() => void handleSaveProfile()}
            >
              Сохранить в профиль
            </button>
            <button
              type="button"
              className="btn btn-outline btn-sm"
              disabled={busySave || running}
              onClick={() => {
                setShowSaveAs((v) => !v);
                setSaveAsName(selectedProfile ? `${selectedProfile.name} (копия)` : "Новый профиль");
              }}
            >
              Сохранить в новый профиль
            </button>
          </div>

          {showSaveAs ? (
            <div className="flex flex-wrap items-end gap-2">
              <label className="form-control">
                <span className="label-text text-xs">Имя нового профиля</span>
                <input
                  className="input input-bordered input-sm"
                  value={saveAsName}
                  onChange={(e) => setSaveAsName(e.target.value)}
                />
              </label>
              <button
                type="button"
                className="btn btn-primary btn-sm"
                disabled={busySave}
                onClick={() => void handleSaveAs()}
              >
                Создать
              </button>
            </div>
          ) : null}

          {dirty ? (
            <p className="text-xs opacity-70">
              Настройки на доске отличаются от профиля «{selectedProfile?.name}».
            </p>
          ) : null}
        </>
      ) : (
        <p className="text-sm opacity-70">
          Выберите профиль — его настройки скопируются сюда. Дальше можно менять
          их только для текущей доски.
        </p>
      )}

      {lines.length > 0 ? (
        <div className="overflow-x-auto">
          <table className="table table-zebra table-sm">
            <thead>
              <tr>
                <th>#</th>
                <th>Оценка</th>
                <th>Depth</th>
                <th>PV</th>
              </tr>
            </thead>
            <tbody>
              {lines.map((line) => (
                <tr key={line.multipv}>
                  <td>{line.multipv}</td>
                  <td className="font-mono">{formatScore(line, turn)}</td>
                  <td>{line.depth}</td>
                  <td className="font-mono text-xs">{line.pv.join(" ")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </section>
  );
}
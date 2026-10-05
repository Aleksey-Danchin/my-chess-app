import { useCallback, useEffect, useState } from "react";
import { AnalysisPanel } from "./components/AnalysisPanel";
import { ChessBoard } from "./components/ChessBoard";
import { DatabasesPage } from "./components/DatabasesPage";
import { EnginesPage } from "./components/EnginesPage";
import { GameStatus } from "./components/GameStatus";
import { PromotionDialog } from "./components/PromotionDialog";
import { RepertoiresPage } from "./components/RepertoiresPage";
import {
  getPosition,
  makeMove,
  resetGame,
  type PositionSnapshot,
  type PromotionPiece,
} from "./lib/game";

type Tab = "board" | "engines" | "databases" | "repertoires";

type PendingPromotion = {
  from: string;
  to: string;
};

function BoardTab() {
  const [position, setPosition] = useState<PositionSnapshot | null>(null);
  const [pendingPromotion, setPendingPromotion] =
    useState<PendingPromotion | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadPosition = useCallback(async () => {
    try {
      const snap = await getPosition();
      setPosition(snap);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void loadPosition();
  }, [loadPosition]);

  const applySnapshot = (snap: PositionSnapshot) => {
    setPosition(snap);
    if (snap.needsPromotion) {
      return true;
    }
    setPendingPromotion(null);
    return false;
  };

  const handleUserMove = async (from: string, to: string) => {
    if (busy || pendingPromotion) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const snap = await makeMove(from, to);
      const needsPromo = applySnapshot(snap);
      if (needsPromo) {
        setPendingPromotion({ from, to });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      await loadPosition();
    } finally {
      setBusy(false);
    }
  };

  const handlePromotion = async (piece: PromotionPiece) => {
    if (!pendingPromotion) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const snap = await makeMove(
        pendingPromotion.from,
        pendingPromotion.to,
        piece,
      );
      applySnapshot(snap);
      setPendingPromotion(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPendingPromotion(null);
      await loadPosition();
    } finally {
      setBusy(false);
    }
  };

  const handleCancelPromotion = async () => {
    setPendingPromotion(null);
    await loadPosition();
  };

  const handleNewGame = async () => {
    setBusy(true);
    setError(null);
    setPendingPromotion(null);
    try {
      const snap = await resetGame();
      setPosition(snap);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <section className="rounded-box bg-base-100 p-4 shadow-sm">
        <GameStatus
          position={position}
          onNewGame={() => void handleNewGame()}
          busy={busy}
        />
      </section>

      {error ? (
        <div role="alert" className="alert alert-error text-sm">
          <span>{error}</span>
        </div>
      ) : null}

      <section className="flex flex-1 flex-col items-stretch justify-start gap-6 lg:flex-row lg:items-start">
        <div className="flex flex-1 justify-center">
          {position ? (
            <ChessBoard
              position={position}
              interactive={!busy && !pendingPromotion}
              onUserMove={(from, to) => void handleUserMove(from, to)}
            />
          ) : (
            <span className="loading loading-spinner loading-lg" />
          )}
        </div>
        <div className="w-full lg:max-w-md">
          <AnalysisPanel
            fen={position?.fen ?? null}
            turn={position?.turn ?? "white"}
          />
        </div>
      </section>

      <PromotionDialog
        open={pendingPromotion !== null}
        turn={position?.turn ?? "white"}
        onChoose={(piece) => void handlePromotion(piece)}
        onCancel={() => void handleCancelPromotion()}
      />
    </>
  );
}

function App() {
  const [tab, setTab] = useState<Tab>("board");

  return (
    <main className="min-h-full bg-base-200">
      <div className="mx-auto flex min-h-full max-w-6xl flex-col gap-6 p-4 md:p-6">
        <header className="space-y-3">
          <div className="space-y-1">
            <h1 className="text-2xl font-bold tracking-tight">my-chess-app</h1>
            <p className="text-sm opacity-70">
              Доска, движки, базы и автосборщик репертуаров
            </p>
          </div>
          <div role="tablist" className="tabs tabs-box w-fit flex-wrap">
            <button
              type="button"
              role="tab"
              className={`tab ${tab === "board" ? "tab-active" : ""}`}
              onClick={() => setTab("board")}
            >
              Доска
            </button>
            <button
              type="button"
              role="tab"
              className={`tab ${tab === "engines" ? "tab-active" : ""}`}
              onClick={() => setTab("engines")}
            >
              Движки
            </button>
            <button
              type="button"
              role="tab"
              className={`tab ${tab === "databases" ? "tab-active" : ""}`}
              onClick={() => setTab("databases")}
            >
              Базы
            </button>
            <button
              type="button"
              role="tab"
              className={`tab ${tab === "repertoires" ? "tab-active" : ""}`}
              onClick={() => setTab("repertoires")}
            >
              Репертуары
            </button>
          </div>
        </header>

        {tab === "board" ? <BoardTab /> : null}
        {tab === "engines" ? <EnginesPage /> : null}
        {tab === "databases" ? <DatabasesPage /> : null}
        {tab === "repertoires" ? <RepertoiresPage /> : null}
      </div>
    </main>
  );
}

export default App;

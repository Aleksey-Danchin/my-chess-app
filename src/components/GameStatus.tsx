import type { PositionSnapshot } from "../lib/game";

type Props = {
  position: PositionSnapshot | null;
  onNewGame: () => void;
  busy?: boolean;
};

function statusLabel(position: PositionSnapshot): string {
  if (position.status === "checkmate") {
    const winner = position.turn === "white" ? "чёрные" : "белые";
    return `Мат — победили ${winner}`;
  }
  if (position.status === "stalemate") {
    return "Пат — ничья";
  }
  const turn = position.turn === "white" ? "белых" : "чёрных";
  if (position.check) {
    return `Ход ${turn} — шах`;
  }
  return `Ход ${turn}`;
}

export function GameStatus({ position, onNewGame, busy }: Props) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p className="text-sm opacity-70">Партия</p>
        <p className="text-lg font-semibold">
          {position ? statusLabel(position) : "Загрузка…"}
        </p>
      </div>
      <button
        type="button"
        className="btn btn-primary"
        onClick={onNewGame}
        disabled={busy}
      >
        Новая партия
      </button>
    </div>
  );
}

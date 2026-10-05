import type { PromotionPiece } from "../lib/game";

type Props = {
  open: boolean;
  turn: "white" | "black";
  onChoose: (piece: PromotionPiece) => void;
  onCancel: () => void;
};

const OPTIONS: { piece: PromotionPiece; label: string }[] = [
  { piece: "q", label: "Ферзь" },
  { piece: "r", label: "Ладья" },
  { piece: "b", label: "Слон" },
  { piece: "n", label: "Конь" },
];

export function PromotionDialog({ open, turn, onChoose, onCancel }: Props) {
  if (!open) {
    return null;
  }

  return (
    <dialog className="modal modal-open">
      <div className="modal-box">
        <h3 className="text-lg font-bold">Превращение пешки</h3>
        <p className="py-2 opacity-80">
          Выберите фигуру ({turn === "white" ? "белые" : "чёрные"})
        </p>
        <div className="flex flex-wrap gap-2 pt-2">
          {OPTIONS.map(({ piece, label }) => (
            <button
              key={piece}
              type="button"
              className="btn btn-outline"
              onClick={() => onChoose(piece)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="modal-action">
          <button type="button" className="btn" onClick={onCancel}>
            Отмена
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button type="button" onClick={onCancel}>
          close
        </button>
      </form>
    </dialog>
  );
}

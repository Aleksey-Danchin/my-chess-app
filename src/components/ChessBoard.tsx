import { useEffect, useRef } from "react";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Key } from "@lichess-org/chessground/types";
import type { Config } from "@lichess-org/chessground/config";
import type { PositionSnapshot } from "../lib/game";

import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";

type Props = {
  position: PositionSnapshot;
  interactive: boolean;
  onUserMove: (from: string, to: string) => void;
};

function toDests(dests: Record<string, string[]>): Map<Key, Key[]> {
  const map = new Map<Key, Key[]>();
  for (const [from, tos] of Object.entries(dests)) {
    map.set(from as Key, tos as Key[]);
  }
  return map;
}

function boardConfig(
  position: PositionSnapshot,
  interactive: boolean,
  onUserMove: (from: string, to: string) => void,
): Config {
  const playing = interactive && position.status === "playing";
  return {
    fen: position.fen,
    turnColor: position.turn,
    check: position.check,
    lastMove: position.lastMove
      ? (position.lastMove as [Key, Key])
      : undefined,
    coordinates: true,
    highlight: { lastMove: true, check: true },
    animation: { enabled: true, duration: 200 },
    movable: {
      free: false,
      color: playing ? position.turn : undefined,
      dests: playing ? toDests(position.dests) : new Map(),
      showDests: true,
      events: {
        after: (orig: Key, dest: Key) => {
          onUserMove(orig, dest);
        },
      },
    },
    premovable: { enabled: false },
    draggable: { enabled: playing },
    selectable: { enabled: playing },
  };
}

export function ChessBoard({ position, interactive, onUserMove }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const apiRef = useRef<Api | null>(null);
  const onMoveRef = useRef(onUserMove);
  onMoveRef.current = onUserMove;

  useEffect(() => {
    const el = hostRef.current;
    if (!el) {
      return;
    }

    const api = Chessground(
      el,
      boardConfig(position, interactive, (from, to) =>
        onMoveRef.current(from, to),
      ),
    );
    apiRef.current = api;

    return () => {
      api.destroy();
      apiRef.current = null;
    };
    // Mount once; later updates go through api.set.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    apiRef.current?.set(
      boardConfig(position, interactive, (from, to) =>
        onMoveRef.current(from, to),
      ),
    );
  }, [position, interactive]);

  return <div ref={hostRef} className="board-host" />;
}

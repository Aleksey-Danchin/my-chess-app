import { useMemo } from "react";

export type GraphNode = {
  key: string;
  depth: number;
};

export type GraphEdge = {
  from: string;
  to: string;
  san: string;
  source: string;
};

type Props = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  currentKey: string | null;
  onSelectKey?: (key: string) => void;
};

const COL_W = 128;
const ROW_H = 44;
const PAD_X = 28;
const PAD_Y = 28;
const NODE_R = 10;
const BARYCENTER_SWEEPS = 6;
const CROSSING_SWEEPS = 8;

type Pos = { x: number; y: number; depth: number };

function orderOf(layer: string[]): Map<string, number> {
  const m = new Map<string, number>();
  layer.forEach((k, i) => m.set(k, i));
  return m;
}

/** Count crossings of edges between two consecutive layers. */
function countCrossings(
  left: string[],
  right: string[],
  pairs: Array<[string, string]>,
): number {
  const L = orderOf(left);
  const R = orderOf(right);
  const oriented: Array<[number, number]> = [];
  for (const [a, b] of pairs) {
    const i = L.get(a);
    const j = R.get(b);
    if (i == null || j == null) continue;
    oriented.push([i, j]);
  }
  let crossings = 0;
  for (let i = 0; i < oriented.length; i++) {
    for (let j = i + 1; j < oriented.length; j++) {
      const [a1, b1] = oriented[i]!;
      const [a2, b2] = oriented[j]!;
      if ((a1 - a2) * (b1 - b2) < 0) crossings += 1;
    }
  }
  return crossings;
}

function barySort(
  layer: string[],
  neighbors: Map<string, string[]>,
  fixedOrder: Map<string, number>,
) {
  const scored = layer.map((key, idx) => {
    const ns = (neighbors.get(key) ?? []).filter((n) => fixedOrder.has(n));
    if (ns.length === 0) {
      return { key, score: idx, tie: key };
    }
    // Median is more stable than mean for crossing reduction.
    const positions = ns
      .map((n) => fixedOrder.get(n)!)
      .sort((a, b) => a - b);
    const mid = Math.floor(positions.length / 2);
    const score =
      positions.length % 2 === 1
        ? positions[mid]!
        : (positions[mid - 1]! + positions[mid]!) / 2;
    return { key, score, tie: key };
  });
  scored.sort((a, b) => a.score - b.score || a.tie.localeCompare(b.tie));
  layer.splice(0, layer.length, ...scored.map((s) => s.key));
}

/**
 * Layered DAG layout: median barycenter + adjacent swaps to cut crossings.
 * Deterministic so selection/re-fetch does not reshuffle nodes.
 */
function layoutGraph(
  nodes: GraphNode[],
  edges: GraphEdge[],
): { pos: Map<string, Pos>; width: number; height: number } {
  if (nodes.length === 0) {
    return { pos: new Map(), width: 200, height: 80 };
  }

  const depthOf = new Map<string, number>();
  for (const n of nodes) {
    depthOf.set(n.key, n.depth);
  }

  const byDepth = new Map<number, string[]>();
  for (const n of nodes) {
    const list = byDepth.get(n.depth) ?? [];
    list.push(n.key);
    byDepth.set(n.depth, list);
  }
  for (const list of byDepth.values()) {
    list.sort((a, b) => a.localeCompare(b));
  }

  const depths = [...byDepth.keys()].sort((a, b) => a - b);

  const preds = new Map<string, string[]>();
  const succs = new Map<string, string[]>();
  /** Edges between consecutive depths only (for crossing math). */
  const layerPairs = new Map<string, Array<[string, string]>>();

  for (const e of edges) {
    const df = depthOf.get(e.from);
    const dt = depthOf.get(e.to);
    if (df == null || dt == null) continue;
    const p = preds.get(e.to) ?? [];
    p.push(e.from);
    preds.set(e.to, p);
    const s = succs.get(e.from) ?? [];
    s.push(e.to);
    succs.set(e.from, s);
    if (dt === df + 1) {
      const key = `${df}->${dt}`;
      const list = layerPairs.get(key) ?? [];
      list.push([e.from, e.to]);
      layerPairs.set(key, list);
    }
  }

  for (let sweep = 0; sweep < BARYCENTER_SWEEPS; sweep++) {
    for (let i = 1; i < depths.length; i++) {
      const prev = byDepth.get(depths[i - 1]!)!;
      const cur = byDepth.get(depths[i]!)!;
      barySort(cur, preds, orderOf(prev));
    }
    for (let i = depths.length - 2; i >= 0; i--) {
      const next = byDepth.get(depths[i + 1]!)!;
      const cur = byDepth.get(depths[i]!)!;
      barySort(cur, succs, orderOf(next));
    }
  }

  // Adjacent transposition: swap neighbors if that reduces crossings.
  for (let sweep = 0; sweep < CROSSING_SWEEPS; sweep++) {
    let improved = false;
    for (let i = 0; i < depths.length - 1; i++) {
      const left = byDepth.get(depths[i]!)!;
      const right = byDepth.get(depths[i + 1]!)!;
      const pairs = layerPairs.get(`${depths[i]}->${depths[i + 1]}`) ?? [];
      if (pairs.length < 2 || right.length < 2) continue;

      for (let r = 0; r < right.length - 1; r++) {
        const before = countCrossings(left, right, pairs);
        const tmp = right[r]!;
        right[r] = right[r + 1]!;
        right[r + 1] = tmp;
        const after = countCrossings(left, right, pairs);
        if (after < before) {
          improved = true;
        } else {
          right[r + 1] = right[r]!;
          right[r] = tmp;
        }
      }
    }
    // Also optimize left layer vs next (right-to-left adjacent swaps).
    for (let i = depths.length - 1; i >= 1; i--) {
      const left = byDepth.get(depths[i - 1]!)!;
      const right = byDepth.get(depths[i]!)!;
      const pairs = layerPairs.get(`${depths[i - 1]}->${depths[i]}`) ?? [];
      if (pairs.length < 2 || left.length < 2) continue;

      for (let l = 0; l < left.length - 1; l++) {
        const before = countCrossings(left, right, pairs);
        const tmp = left[l]!;
        left[l] = left[l + 1]!;
        left[l + 1] = tmp;
        const after = countCrossings(left, right, pairs);
        if (after < before) {
          improved = true;
        } else {
          left[l + 1] = left[l]!;
          left[l] = tmp;
        }
      }
    }
    if (!improved) break;
  }

  const pos = new Map<string, Pos>();
  let maxRows = 1;
  for (const d of depths) {
    const col = byDepth.get(d) ?? [];
    maxRows = Math.max(maxRows, col.length);
    col.forEach((key, idx) => {
      pos.set(key, {
        x: PAD_X + d * COL_W,
        y: PAD_Y + idx * ROW_H,
        depth: d,
      });
    });
  }

  const width = PAD_X * 2 + Math.max(0, depths.length - 1) * COL_W + 40;
  const height = PAD_Y * 2 + Math.max(0, maxRows - 1) * ROW_H + 40;
  return { pos, width: Math.max(width, 200), height: Math.max(height, 80) };
}

export function RepertoireGraph({
  nodes,
  edges,
  currentKey,
  onSelectKey,
}: Props) {
  const layout = useMemo(() => layoutGraph(nodes, edges), [nodes, edges]);

  if (nodes.length === 0) {
    return (
      <p className="text-sm opacity-70">Граф появится по мере сборки.</p>
    );
  }

  return (
    <div className="overflow-auto rounded-box border border-base-300 bg-base-200/40">
      <svg
        width={layout.width}
        height={layout.height}
        viewBox={`0 0 ${layout.width} ${layout.height}`}
        className="block min-w-full"
        role="img"
        aria-label="Граф репертуара"
      >
        {edges.map((e, i) => {
          const a = layout.pos.get(e.from);
          const b = layout.pos.get(e.to);
          if (!a || !b) return null;
          const midX = (a.x + b.x) / 2;
          const colorClass =
            e.source === "engine" ? "stroke-primary" : "stroke-secondary";
          // Slight vertical offset on labels when endpoints differ, reduces overlap.
          const labelY =
            (a.y + b.y) / 2 - 4 + (a.y === b.y ? 0 : a.y < b.y ? -2 : 2);
          return (
            <g key={`${e.from}-${e.to}-${e.san}-${i}`}>
              <path
                d={`M ${a.x + NODE_R} ${a.y} C ${midX} ${a.y}, ${midX} ${b.y}, ${b.x - NODE_R} ${b.y}`}
                fill="none"
                className={colorClass}
                strokeWidth={1.5}
                opacity={0.75}
              />
              <text
                x={midX}
                y={labelY}
                textAnchor="middle"
                className="fill-base-content text-[10px]"
                opacity={0.8}
              >
                {e.san}
              </text>
            </g>
          );
        })}
        {[...layout.pos.entries()].map(([key, p]) => {
          const active = key === currentKey;
          return (
            <g
              key={key}
              className={onSelectKey ? "cursor-pointer" : undefined}
              onClick={() => onSelectKey?.(key)}
            >
              <circle
                cx={p.x}
                cy={p.y}
                r={NODE_R}
                className={
                  active
                    ? "fill-primary stroke-primary-content"
                    : "fill-base-100 stroke-base-content"
                }
                strokeWidth={active ? 2.5 : 1.25}
              />
              <text
                x={p.x}
                y={p.y + NODE_R + 12}
                textAnchor="middle"
                className="fill-base-content text-[9px]"
                opacity={0.55}
              >
                d{p.depth}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

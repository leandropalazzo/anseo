"use client";

import { useMemo } from "react";

import type {
  CitationGraph as CitationGraphData,
  CitationGraphNode,
} from "@/lib/api";
import { resolveProviderIdentity } from "@/lib/provider-colors";

export interface CitationGraphProps {
  graph: CitationGraphData;
}

const W = 900;
const H = 520;
const PROVIDERS = new Set<string>([
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
]);

/**
 * Force-directed citation graph. Tiny spring-damper solver (≈40 LOC) —
 * Hooke attraction along edges, Coulomb repulsion between all nodes,
 * fixed iteration count so render is deterministic and SSR-safe.
 *
 * No d3; we render the converged positions as plain SVG circles + lines.
 * Provider nodes use brand colors + the `█` density glyph for the a11y
 * pairing rule (UX-DR). Closes the citation-graph AC for Story 14.2.
 */
export function CitationGraph({ graph }: CitationGraphProps) {
  const layout = useMemo(() => computeLayout(graph), [graph]);

  if (graph.nodes.length === 0) {
    return (
      <div className="px-[14px] py-[24px] text-center font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text-faint)]">
        no citation data
      </div>
    );
  }

  const maxWeight = Math.max(1, ...graph.edges.map((e) => e.weight));

  return (
    <div>
      <svg
        role="img"
        aria-label="Citation graph — providers linked to cited domains"
        viewBox={`0 0 ${W} ${H}`}
        className="block w-full"
      >
        {/* edges */}
        {graph.edges.map((e, i) => {
          const a = layout.get(e.source);
          const b = layout.get(e.target);
          if (!a || !b) return null;
          const isProviderEdge =
            PROVIDERS.has(e.source) || e.source.startsWith("openrouter");
          const stroke = isProviderEdge
            ? resolveProviderIdentity(e.source).cssVar
            : "var(--border-strong)";
          return (
            <line
              key={i}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              stroke={stroke}
              strokeWidth={0.5 + (e.weight / maxWeight) * 2}
              opacity={0.45}
            />
          );
        })}

        {/* nodes */}
        {graph.nodes.map((n) => {
          const pos = layout.get(n.id);
          if (!pos) return null;
          const isProvider = n.kind === "provider";
          const r = isProvider ? 9 : 5;
          const fill = isProvider
            ? resolveProviderIdentity(n.id).cssVar
            : "var(--bg-elev-2)";
          const stroke = isProvider ? fill : "var(--border-strong)";
          const labelX = pos.x + r + 4;
          return (
            <g key={n.id}>
              <circle
                cx={pos.x}
                cy={pos.y}
                r={r}
                fill={fill}
                stroke={stroke}
                strokeWidth={1}
              />
              {isProvider && (
                <text
                  x={pos.x}
                  y={pos.y + 3}
                  textAnchor="middle"
                  fill="var(--bg)"
                  aria-hidden
                  style={{ fontFamily: "var(--font-mono)", fontSize: 8 }}
                >
                  █
                </text>
              )}
              <text
                x={labelX}
                y={pos.y + 3}
                fill="var(--text)"
                style={{
                  fontFamily: "var(--font-mono)",
                  fontSize: isProvider ? 11 : 9,
                  fontWeight: isProvider ? 600 : 400,
                }}
              >
                {n.label}
              </text>
            </g>
          );
        })}
      </svg>
      <div className="mt-[10px] flex flex-wrap gap-[12px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
        <span>
          <span
            aria-hidden
            className="mr-[6px] inline-block align-middle"
            style={{
              width: 10,
              height: 10,
              borderRadius: 999,
              background: "var(--accent)",
            }}
          />
          provider (█ density glyph)
        </span>
        <span>
          <span
            aria-hidden
            className="mr-[6px] inline-block align-middle"
            style={{
              width: 10,
              height: 10,
              borderRadius: 999,
              background: "var(--bg-elev-2)",
              border: "1px solid var(--border-strong)",
            }}
          />
          cited domain
        </span>
        <span>
          {graph.nodes.length} nodes · {graph.edges.length} edges
        </span>
      </div>
    </div>
  );
}

interface Pt {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

/**
 * Spring-damper layout. Hooke attraction along edges, Coulomb repulsion
 * between every node pair, velocity damped each step. Deterministic seed
 * keyed off node ids so SSR and client produce identical SVGs.
 */
function computeLayout(graph: CitationGraphData): Map<string, Pt> {
  const nodes = graph.nodes;
  const pts = new Map<string, Pt>();
  const centerX = W / 2;
  const centerY = H / 2;
  // Seed positions around a circle, providers on inner ring, domains on outer.
  nodes.forEach((n, i) => {
    const isProvider = n.kind === "provider";
    const ring = isProvider ? 60 : 220;
    const angle = (i / Math.max(1, nodes.length)) * Math.PI * 2 + hash(n.id);
    pts.set(n.id, {
      x: centerX + Math.cos(angle) * ring,
      y: centerY + Math.sin(angle) * ring,
      vx: 0,
      vy: 0,
    });
  });

  const adj = new Map<string, Set<string>>();
  for (const e of graph.edges) {
    if (!adj.has(e.source)) adj.set(e.source, new Set());
    if (!adj.has(e.target)) adj.set(e.target, new Set());
    adj.get(e.source)!.add(e.target);
    adj.get(e.target)!.add(e.source);
  }

  const REPULSION = 4500;
  const SPRING_K = 0.012;
  const REST = 120;
  const DAMP = 0.82;
  const STEPS = 220;

  const ids = Array.from(pts.keys());
  for (let step = 0; step < STEPS; step++) {
    // Repulsion
    for (let i = 0; i < ids.length; i++) {
      const a = pts.get(ids[i]!)!;
      for (let j = i + 1; j < ids.length; j++) {
        const b = pts.get(ids[j]!)!;
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const d2 = dx * dx + dy * dy + 0.01;
        const f = REPULSION / d2;
        const d = Math.sqrt(d2);
        const fx = (dx / d) * f;
        const fy = (dy / d) * f;
        a.vx += fx;
        a.vy += fy;
        b.vx -= fx;
        b.vy -= fy;
      }
    }
    // Spring attraction along edges
    for (const e of graph.edges) {
      const a = pts.get(e.source);
      const b = pts.get(e.target);
      if (!a || !b) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const d = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const f = SPRING_K * (d - REST);
      const fx = (dx / d) * f;
      const fy = (dy / d) * f;
      a.vx += fx;
      a.vy += fy;
      b.vx -= fx;
      b.vy -= fy;
    }
    // Integrate + damp + clamp to viewport
    for (const id of ids) {
      const p = pts.get(id)!;
      p.vx *= DAMP;
      p.vy *= DAMP;
      p.x += p.vx;
      p.y += p.vy;
      p.x = Math.max(30, Math.min(W - 30, p.x));
      p.y = Math.max(30, Math.min(H - 30, p.y));
    }
  }
  return pts;
}

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return ((h % 1000) / 1000) * Math.PI;
}

// `CitationGraphNode` is used implicitly; re-export to keep the type in a
// stable namespace for any future callers (e.g. graph filtering UI).
export type { CitationGraphNode };

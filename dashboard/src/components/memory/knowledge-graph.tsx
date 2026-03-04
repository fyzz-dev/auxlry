import { useRef, useEffect, useCallback } from "react";
import type { GraphNode, GraphEdge } from "./types";

// ── Colour maps ──────────────────────────────────────────────────────────────

export const TYPE_COLORS: Record<string, string> = {
  fact: "#60a5fa",
  decision: "#2dd4bf",
  inference: "#a78bfa",
  preference: "#f472b6",
  observation: "#fbbf24",
  event: "#818cf8",
};

export const RELATION_COLORS: Record<string, string> = {
  related_to: "#6b7280",
  caused_by: "#f5a623",
  supports: "#4ecdc4",
  contradicts: "#ef4444",
  elaborates: "#a78bfa",
};

const DEFAULT_COLOR = "#6b7280";

// ── Internal types ───────────────────────────────────────────────────────────

interface Vec2 {
  x: number;
  y: number;
}

interface Particle {
  id: string;
  pos: Vec2;
  vel: Vec2;
  r: number;
  color: string;
  label: string;
  data: GraphNode;
}

interface Link {
  a: Particle;
  b: Particle;
  color: string;
}

interface Sim {
  particles: Particle[];
  links: Link[];
  alpha: number;
}
interface View {
  tx: number;
  ty: number;
  scale: number;
}

// ── Physics ──────────────────────────────────────────────────────────────────

function stepSim(sim: Sim) {
  const { particles, links, alpha } = sim;
  const len = particles.length;

  // Repulsion between all node pairs
  for (let i = 0; i < len; i++) {
    for (let j = i + 1; j < len; j++) {
      const dx = particles[j].pos.x - particles[i].pos.x;
      const dy = particles[j].pos.y - particles[i].pos.y;
      const d2 = dx * dx + dy * dy || 0.01;
      const d = Math.sqrt(d2);
      const minD = particles[i].r + particles[j].r + 20;
      if (d < minD * 5) {
        const f = (minD * minD * 2.5 * alpha) / d2;
        const nx = dx / d,
          ny = dy / d;
        particles[i].vel.x -= nx * f;
        particles[i].vel.y -= ny * f;
        particles[j].vel.x += nx * f;
        particles[j].vel.y += ny * f;
      }
    }
  }

  // Spring attraction along edges
  const restLen = 110;
  for (const l of links) {
    const dx = l.b.pos.x - l.a.pos.x;
    const dy = l.b.pos.y - l.a.pos.y;
    const d = Math.sqrt(dx * dx + dy * dy) || 1;
    const f = (d - restLen) * 0.025 * alpha;
    const nx = dx / d,
      ny = dy / d;
    l.a.vel.x += nx * f;
    l.a.vel.y += ny * f;
    l.b.vel.x -= nx * f;
    l.b.vel.y -= ny * f;
  }

  // Weak gravity toward origin
  for (const p of particles) {
    p.vel.x -= p.pos.x * 0.005 * alpha;
    p.vel.y -= p.pos.y * 0.005 * alpha;
  }

  // Integrate + dampen
  for (const p of particles) {
    p.vel.x *= 0.78;
    p.vel.y *= 0.78;
    p.pos.x += p.vel.x;
    p.pos.y += p.vel.y;
  }
}

// ── Rendering ────────────────────────────────────────────────────────────────

function drawScene(
  ctx: CanvasRenderingContext2D,
  physW: number,
  physH: number,
  dpr: number,
  sim: Sim,
  view: View,
  hoveredId: string | null,
  selectedId: string | null,
) {
  const W = physW / dpr;
  const H = physH / dpr;
  const { tx, ty, scale } = view;

  ctx.clearRect(0, 0, physW, physH);
  ctx.save();
  ctx.scale(dpr, dpr);
  ctx.translate(tx + W / 2, ty + H / 2);
  ctx.scale(scale, scale);

  // Edges
  ctx.lineWidth = 1.5 / scale;
  for (const l of sim.links) {
    ctx.beginPath();
    ctx.moveTo(l.a.pos.x, l.a.pos.y);
    ctx.lineTo(l.b.pos.x, l.b.pos.y);
    ctx.strokeStyle = l.color + "55";
    ctx.stroke();
  }

  // Nodes
  for (const p of sim.particles) {
    const hovered = hoveredId === p.id;
    const selected = selectedId === p.id;
    const hot = hovered || selected;

    // Outer glow ring
    if (hot) {
      ctx.beginPath();
      ctx.arc(
        p.pos.x,
        p.pos.y,
        p.r + (selected ? 8 : 5) / scale,
        0,
        Math.PI * 2,
      );
      ctx.fillStyle = p.color + (selected ? "32" : "1a");
      ctx.fill();
    }

    // Node fill
    ctx.beginPath();
    ctx.arc(p.pos.x, p.pos.y, p.r, 0, Math.PI * 2);
    ctx.fillStyle = hot ? p.color : p.color + "bb";
    ctx.fill();
  }

  // Labels — only when zoomed in enough
  if (scale >= 0.45) {
    const fs = Math.max(9, 10 / scale);
    ctx.font = `${fs}px system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (const p of sim.particles) {
      const hot = hoveredId === p.id || selectedId === p.id;
      ctx.fillStyle = hot
        ? "rgba(255,255,255,0.9)"
        : "rgba(255,255,255,0.45)";
      ctx.fillText(p.label, p.pos.x, p.pos.y + p.r + 3 / scale);
    }
  }

  ctx.restore();
}

// ── Component ────────────────────────────────────────────────────────────────

export interface KnowledgeGraphProps {
  nodes: GraphNode[];
  edges: GraphEdge[];
  onNodeClick: (node: GraphNode | null) => void;
}

export function KnowledgeGraph({
  nodes,
  edges,
  onNodeClick,
}: KnowledgeGraphProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<Sim>({ particles: [], links: [], alpha: 0 });
  const viewRef = useRef<View>({ tx: 0, ty: 0, scale: 1 });
  const dprRef = useRef(1);
  const hoveredRef = useRef<string | null>(null);
  const selectedRef = useRef<string | null>(null);
  const onClickRef = useRef(onNodeClick);
  const pointerRef = useRef({
    down: false,
    moved: false,
    startX: 0,
    startY: 0,
    dragNode: null as Particle | null,
  });

  useEffect(() => {
    onClickRef.current = onNodeClick;
  }, [onNodeClick]);

  // Init simulation whenever data changes
  useEffect(() => {
    const count = nodes.length;
    const particles: Particle[] = nodes.map((n, i) => {
      const angle = (i / Math.max(count, 1)) * Math.PI * 2;
      const radius = 50 + Math.sqrt(count) * 18;
      const label =
        n.content.length > 26 ? n.content.slice(0, 26) + "\u2026" : n.content;
      return {
        id: n.id,
        pos: {
          x:
            Math.cos(angle) * radius + (Math.random() - 0.5) * 10,
          y:
            Math.sin(angle) * radius + (Math.random() - 0.5) * 10,
        },
        vel: { x: 0, y: 0 },
        r: 4 + Math.min(n.access_count, 10) * 0.8,
        color: TYPE_COLORS[n.memory_type] ?? DEFAULT_COLOR,
        label,
        data: n,
      };
    });

    const pMap = new Map(particles.map((p) => [p.id, p]));
    const links: Link[] = edges
      .map((e) => ({
        a: pMap.get(e.source),
        b: pMap.get(e.target),
        color: RELATION_COLORS[e.relation_type] ?? DEFAULT_COLOR,
      }))
      .filter((l): l is Link => !!l.a && !!l.b);

    simRef.current = { particles, links, alpha: 1 };
    selectedRef.current = null;

    // Auto-fit: compute scale so all nodes are visible
    if (particles.length > 0) {
      // Run a few simulation steps to settle layout before fitting
      const warmSim = { particles, links, alpha: 1 };
      for (let i = 0; i < 120; i++) {
        stepSim(warmSim);
        warmSim.alpha = Math.max(0, warmSim.alpha - 0.003);
      }

      const container = containerRef.current;
      const W = container?.clientWidth ?? 800;
      const H = container?.clientHeight ?? 600;
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
      for (const p of particles) {
        minX = Math.min(minX, p.pos.x - p.r);
        maxX = Math.max(maxX, p.pos.x + p.r);
        minY = Math.min(minY, p.pos.y - p.r);
        maxY = Math.max(maxY, p.pos.y + p.r);
      }
      const graphW = maxX - minX || 1;
      const graphH = maxY - minY || 1;
      const padding = 60;
      const scale = Math.min(
        (W - padding * 2) / graphW,
        (H - padding * 2) / graphH,
        2,
      );
      const cx = (minX + maxX) / 2;
      const cy = (minY + maxY) / 2;
      viewRef.current = { tx: -cx * scale, ty: -cy * scale, scale };
    } else {
      viewRef.current = { tx: 0, ty: 0, scale: 1 };
    }
  }, [nodes, edges]);

  // Canvas resize
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      dprRef.current = dpr;
      canvas.width = container.clientWidth * dpr;
      canvas.height = container.clientHeight * dpr;
      canvas.style.width = "100%";
      canvas.style.height = "100%";
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  // RAF loop
  useEffect(() => {
    let rafId = 0;
    const tick = () => {
      const sim = simRef.current;
      if (sim.alpha > 0.005) {
        stepSim(sim);
        sim.alpha = Math.max(0, sim.alpha - 0.003);
      }
      const canvas = canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext("2d");
        if (ctx) {
          drawScene(
            ctx,
            canvas.width,
            canvas.height,
            dprRef.current,
            sim,
            viewRef.current,
            hoveredRef.current,
            selectedRef.current,
          );
        }
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, []);

  // Wheel zoom
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const W = rect.width,
        H = rect.height;
      const { tx, ty, scale } = viewRef.current;
      const factor = e.deltaY < 0 ? 1.1 : 0.9;
      const newScale = Math.max(0.1, Math.min(8, scale * factor));
      const ratio = newScale / scale;
      viewRef.current = {
        scale: newScale,
        tx: (mx - W / 2) * (1 - ratio) + tx * ratio,
        ty: (my - H / 2) * (1 - ratio) + ty * ratio,
      };
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, []);

  // Hit test
  const hitTest = useCallback(
    (clientX: number, clientY: number): Particle | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const { tx, ty, scale } = viewRef.current;
      const wx = (clientX - rect.left - rect.width / 2 - tx) / scale;
      const wy = (clientY - rect.top - rect.height / 2 - ty) / scale;
      for (const p of simRef.current.particles) {
        const dx = p.pos.x - wx,
          dy = p.pos.y - wy;
        if (dx * dx + dy * dy <= (p.r + 5) * (p.r + 5)) return p;
      }
      return null;
    },
    [],
  );

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const hit = hitTest(e.clientX, e.clientY);
      pointerRef.current = {
        down: true,
        moved: false,
        startX: e.clientX,
        startY: e.clientY,
        dragNode: hit,
      };
    },
    [hitTest],
  );

  const onMouseMove = useCallback(
    (e: React.MouseEvent) => {
      const ptr = pointerRef.current;
      if (ptr.down) {
        const dx = e.clientX - ptr.startX;
        const dy = e.clientY - ptr.startY;
        if (!ptr.moved && (Math.abs(dx) > 2 || Math.abs(dy) > 2))
          ptr.moved = true;
        if (ptr.moved) {
          if (ptr.dragNode) {
            ptr.dragNode.pos.x += dx / viewRef.current.scale;
            ptr.dragNode.pos.y += dy / viewRef.current.scale;
            ptr.dragNode.vel.x = 0;
            ptr.dragNode.vel.y = 0;
            simRef.current.alpha = Math.max(simRef.current.alpha, 0.3);
          } else {
            viewRef.current.tx += dx;
            viewRef.current.ty += dy;
          }
          ptr.startX = e.clientX;
          ptr.startY = e.clientY;
        }
      } else {
        const hit = hitTest(e.clientX, e.clientY);
        hoveredRef.current = hit?.id ?? null;
        if (canvasRef.current) {
          canvasRef.current.style.cursor = hit ? "pointer" : "default";
        }
      }
    },
    [hitTest],
  );

  const onMouseUp = useCallback(
    (e: React.MouseEvent) => {
      const ptr = pointerRef.current;
      if (!ptr.moved) {
        const hit = hitTest(e.clientX, e.clientY);
        if (hit) {
          selectedRef.current = hit.id;
          onClickRef.current(hit.data);
        } else {
          selectedRef.current = null;
          onClickRef.current(null);
        }
      }
      ptr.down = false;
      ptr.dragNode = null;
    },
    [hitTest],
  );

  const onMouseLeave = useCallback(() => {
    pointerRef.current.down = false;
    pointerRef.current.dragNode = null;
    hoveredRef.current = null;
  }, []);

  if (nodes.length === 0) return null;

  return (
    <div ref={containerRef} className="absolute inset-0">
      <canvas
        ref={canvasRef}
        className="block"
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={onMouseLeave}
      />
    </div>
  );
}

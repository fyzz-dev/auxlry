import { TYPE_COLORS } from "./knowledge-graph";
import type { GraphNode } from "./types";

const ALL_TYPES = Object.keys(TYPE_COLORS);

export function GraphLegend({ nodes }: { nodes: GraphNode[] }) {
  const present = ALL_TYPES.filter((t) =>
    nodes.some((n) => n.memory_type === t),
  );
  if (present.length === 0) return null;

  return (
    <div className="absolute bottom-3 left-3 z-10 rounded-xl border border-border/50 bg-card/70 p-3 backdrop-blur-xl text-xs">
      <p className="mb-2 uppercase tracking-wider text-[10px] text-muted-foreground font-medium">
        Key
      </p>
      <div className="grid grid-cols-2 gap-x-4 gap-y-1">
        {present.map((t) => (
          <div key={t} className="flex items-center gap-1.5">
            <span
              className="inline-block size-2 rounded-full shrink-0"
              style={{ background: TYPE_COLORS[t] ?? "#6b7280" }}
            />
            <span className="text-muted-foreground">{t}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

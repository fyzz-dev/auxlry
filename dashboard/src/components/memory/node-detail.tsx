import { AnimatePresence, motion } from "motion/react";
import { X } from "lucide-react";
import { TYPE_COLORS, RELATION_COLORS } from "./knowledge-graph";
import type { GraphNode, GraphEdge } from "./types";

interface NodeDetailProps {
  node: GraphNode | null;
  allEdges: GraphEdge[];
  allNodes: GraphNode[];
  onClose: () => void;
}

export function NodeDetail({
  node,
  allEdges,
  allNodes,
  onClose,
}: NodeDetailProps) {
  return (
    <AnimatePresence>
      {node && (
        <motion.div
          key={node.id}
          initial={{ x: "100%", opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0 }}
          transition={{ type: "spring", damping: 28, stiffness: 220 }}
          className="absolute right-0 top-0 bottom-0 z-20 w-80 overflow-y-auto border-l border-border/50 bg-card/80 backdrop-blur-xl p-4 space-y-4"
        >
          {/* Header */}
          <div className="flex items-start justify-between gap-2">
            <span
              className="inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium"
              style={{
                borderColor:
                  (TYPE_COLORS[node.memory_type] ?? "#6b7280") + "60",
                color: TYPE_COLORS[node.memory_type] ?? "#6b7280",
                background:
                  (TYPE_COLORS[node.memory_type] ?? "#6b7280") + "18",
              }}
            >
              {node.memory_type}
            </span>
            <button
              onClick={onClose}
              className="shrink-0 rounded-lg p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            >
              <X size={14} />
            </button>
          </div>

          {/* Content */}
          <p className="text-sm text-foreground leading-relaxed">
            {node.content}
          </p>

          {/* Meta */}
          <div className="space-y-1.5 text-xs rounded-lg bg-muted/50 p-3">
            <MetaRow label="ID" value={node.id.slice(0, 8) + "\u2026"} />
            <MetaRow label="Created" value={formatDate(node.created_at)} />
            <MetaRow label="Accessed" value={`${node.access_count}\u00d7`} />
          </div>

          {/* Connected edges */}
          <ConnectedEdges
            node={node}
            allEdges={allEdges}
            allNodes={allNodes}
          />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function MetaRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium truncate">{value}</span>
    </div>
  );
}

function ConnectedEdges({
  node,
  allEdges,
  allNodes,
}: {
  node: GraphNode;
  allEdges: GraphEdge[];
  allNodes: GraphNode[];
}) {
  const connected = allEdges.filter(
    (e) => e.source === node.id || e.target === node.id,
  );
  if (connected.length === 0) return null;

  const nodeMap = new Map(allNodes.map((n) => [n.id, n]));

  return (
    <div>
      <p className="mb-2 text-xs font-medium text-muted-foreground uppercase tracking-wider">
        Connections ({connected.length})
      </p>
      <div className="space-y-2">
        {connected.map((e, i) => {
          const otherId = e.source === node.id ? e.target : e.source;
          const direction = e.source === node.id ? "\u2192" : "\u2190";
          const other = nodeMap.get(otherId);
          const relColor = RELATION_COLORS[e.relation_type] ?? "#4b5563";
          return (
            <div
              key={i}
              className="rounded-lg bg-muted/40 p-2.5 text-xs space-y-1.5"
            >
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground">{direction}</span>
                <span
                  className="rounded-full border px-2 py-0.5"
                  style={{ borderColor: relColor + "60", color: relColor }}
                >
                  {e.relation_type.replace("_", " ")}
                </span>
              </div>
              <p className="text-muted-foreground leading-snug line-clamp-2">
                {other?.content ?? otherId}
              </p>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return iso;
  }
}

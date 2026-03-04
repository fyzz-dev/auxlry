import { createFileRoute } from "@tanstack/react-router";
import { useMemoryGraph } from "@/lib/queries";
import { useState } from "react";
import { motion } from "motion/react";
import { KnowledgeGraph } from "@/components/memory/knowledge-graph";
import { NodeDetail } from "@/components/memory/node-detail";
import { GraphLegend } from "@/components/memory/graph-legend";
import type { GraphNode } from "@/components/memory/types";

export const Route = createFileRoute("/memory")({
  component: MemoryPage,
});

function MemoryPage() {
  const { data, isLoading } = useMemoryGraph();
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [search, setSearch] = useState("");

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        Loading graph...
      </div>
    );
  }

  const allNodes = (data?.nodes ?? []) as GraphNode[];
  const allEdges = data?.links ?? [];

  const q = search.trim().toLowerCase();
  const filteredNodes = q
    ? allNodes.filter(
        (n) =>
          n.content.toLowerCase().includes(q) ||
          n.id.toLowerCase().includes(q) ||
          n.memory_type.toLowerCase().includes(q),
      )
    : allNodes;
  const nodeIds = new Set(filteredNodes.map((n) => n.id));
  const filteredEdges = allEdges.filter(
    (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
  );

  return (
    <div className="-m-6 flex flex-col h-[calc(100vh-3rem)]">
      <div className="relative flex-1 overflow-hidden">
        <KnowledgeGraph
          nodes={filteredNodes}
          edges={filteredEdges}
          onNodeClick={setSelectedNode}
        />

        <GraphLegend nodes={filteredNodes} />

        <NodeDetail
          node={selectedNode}
          allEdges={allEdges}
          allNodes={allNodes}
          onClose={() => setSelectedNode(null)}
        />

        {/* Search bar */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 z-10"
        >
          <input
            type="text"
            placeholder="Search memories..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-72 px-4 py-2 rounded-lg bg-background/80 backdrop-blur border border-border text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
          />
        </motion.div>
      </div>
    </div>
  );
}

import { createFileRoute } from "@tanstack/react-router";
import { motion } from "motion/react";
import {
  useMemoryActions,
  useAgentSpawns,
  useMessageHeatmap,
  useMemoryCategories,
} from "@/lib/queries";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { MemoryGrowthChart } from "@/components/charts/MemoryGrowthChart";
import { MessageHeatmap } from "@/components/charts/MessageHeatmap";
import { AgentActivityChart } from "@/components/charts/AgentActivityChart";
import { MemoryTypesRadar } from "@/components/charts/MemoryTypesRadar";
import { EventFeed } from "@/components/charts/EventFeed";

export const Route = createFileRoute("/")({
  component: Overview,
});

function AnimatedCard({
  title,
  children,
  className = "",
  delay = 0,
}: {
  title: string;
  children: React.ReactNode;
  className?: string;
  delay?: number;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay, duration: 0.35, ease: "easeOut" }}
      className={className}
    >
      <Card className="h-full">
        <CardHeader className="pb-0">
          <CardTitle className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            {title}
          </CardTitle>
        </CardHeader>
        <CardContent className="flex-1 min-h-0">{children}</CardContent>
      </Card>
    </motion.div>
  );
}

function Overview() {
  const memoryActions = useMemoryActions();
  const agentSpawns = useAgentSpawns();
  const heatmap = useMessageHeatmap();
  const categories = useMemoryCategories();

  return (
    <div className="mx-auto max-w-screen-xl space-y-4">
      {/* Row 1: Memory growth — full width */}
      <AnimatedCard title="Memory Growth" className="h-56 sm:h-64" delay={0}>
        <div className="h-full">
          <MemoryGrowthChart data={memoryActions.data ?? []} />
        </div>
      </AnimatedCard>

      {/* Row 2: Heatmap (2/3) + Agent Activity (1/3) */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <AnimatedCard
          title="Agent Activity"
          className="lg:col-span-2 h-56"
          delay={0.1}
        >
          <div className="h-full">
            <AgentActivityChart data={agentSpawns.data ?? []} />
          </div>
        </AnimatedCard>
        <AnimatedCard title="Message Activity" className="h-56" delay={0.05}>
          <div className="h-full">
            <MessageHeatmap data={heatmap.data ?? []} />
          </div>
        </AnimatedCard>
      </div>

      {/* Row 3: Radar (1/3) + Event Feed (2/3) */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <AnimatedCard title="Memory Types" className="h-64" delay={0.15}>
          <div className="h-full">
            <MemoryTypesRadar data={categories.data ?? []} />
          </div>
        </AnimatedCard>
        <AnimatedCard
          title="Live Events"
          className="lg:col-span-2 h-64"
          delay={0.2}
        >
          <div className="h-full">
            <EventFeed />
          </div>
        </AnimatedCard>
      </div>
    </div>
  );
}

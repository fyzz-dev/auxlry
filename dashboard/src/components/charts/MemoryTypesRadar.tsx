import { Radar, RadarChart, PolarGrid, PolarAngleAxis } from "recharts";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";

const chartConfig = {
  count: {
    label: "Count",
    color: "var(--color-chart-1)",
  },
} satisfies ChartConfig;

interface Row {
  type: string;
  count: number;
}

interface Props {
  data: Row[];
}

export function MemoryTypesRadar({ data }: Props) {
  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        No data yet
      </div>
    );
  }

  return (
    <ChartContainer config={chartConfig} className="h-full w-full aspect-auto">
      <RadarChart data={data} outerRadius="70%">
        <PolarGrid />
        <PolarAngleAxis dataKey="type" />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Radar
          dataKey="count"
          stroke="var(--color-count)"
          fill="var(--color-count)"
          fillOpacity={0.2}
          strokeWidth={2}
        />
      </RadarChart>
    </ChartContainer>
  );
}

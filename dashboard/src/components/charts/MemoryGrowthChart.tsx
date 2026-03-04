import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";

const chartConfig = {
  count: {
    label: "Memories",
    color: "var(--color-chart-1)",
  },
} satisfies ChartConfig;

interface Props {
  data: { date: string; count: number }[];
}

export function MemoryGrowthChart({ data }: Props) {
  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        No data yet
      </div>
    );
  }

  const maxCount = Math.max(...data.map((d) => d.count), 1);

  return (
    <ChartContainer config={chartConfig} className="h-full w-full aspect-auto">
      <AreaChart data={data}>
        <defs>
          <linearGradient id="memGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--color-count)" stopOpacity={0.3} />
            <stop offset="100%" stopColor="var(--color-count)" stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="date"
          tickLine={false}
          axisLine={false}
          tickFormatter={(v: string) => v.slice(5)}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          domain={[0, maxCount]}
          allowDecimals={false}
        />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Area
          type="monotone"
          dataKey="count"
          stroke="var(--color-count)"
          strokeWidth={2}
          fill="url(#memGrad)"
        />
      </AreaChart>
    </ChartContainer>
  );
}

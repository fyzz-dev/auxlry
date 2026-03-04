import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartLegend,
  ChartLegendContent,
} from "@/components/ui/chart";

const chartConfig = {
  synapse_started: {
    label: "Synapse",
    color: "var(--color-chart-1)",
  },
  operator_started: {
    label: "Operator",
    color: "var(--color-chart-2)",
  },
} satisfies ChartConfig;

interface Row {
  date: string;
  kind?: string;
  count: number;
}

interface Props {
  data: Row[];
}

export function AgentActivityChart({ data }: Props) {
  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        No data yet
      </div>
    );
  }

  const byDate = new Map<string, Record<string, number>>();
  for (const row of data) {
    const entry = byDate.get(row.date) ?? {};
    entry[row.kind ?? "unknown"] = row.count;
    byDate.set(row.date, entry);
  }
  const chartData = Array.from(byDate.entries()).map(([date, counts]) => ({
    date,
    ...counts,
  }));

  return (
    <ChartContainer config={chartConfig} className="h-full w-full aspect-auto">
      <AreaChart data={chartData}>
        <defs>
          <linearGradient id="synGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--color-synapse_started)" stopOpacity={0.25} />
            <stop offset="100%" stopColor="var(--color-synapse_started)" stopOpacity={0} />
          </linearGradient>
          <linearGradient id="opGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--color-operator_started)" stopOpacity={0.25} />
            <stop offset="100%" stopColor="var(--color-operator_started)" stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="date"
          tickLine={false}
          axisLine={false}
          tickFormatter={(v: string) => {
            const d = new Date(v.replace(" ", "T"));
            if (isNaN(d.getTime())) return v.slice(5);
            return d.toLocaleDateString("en", { month: "short", day: "numeric" }) + " " + v.slice(11, 16);
          }}
        />
        <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <ChartLegend content={<ChartLegendContent />} />
        <Area
          type="monotone"
          dataKey="synapse_started"
          stroke="var(--color-synapse_started)"
          strokeWidth={2}
          fill="url(#synGrad)"
        />
        <Area
          type="monotone"
          dataKey="operator_started"
          stroke="var(--color-operator_started)"
          strokeWidth={2}
          fill="url(#opGrad)"
        />
      </AreaChart>
    </ChartContainer>
  );
}

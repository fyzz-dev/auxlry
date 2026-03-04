interface Row {
  date: string;
  count: number;
}

interface Props {
  data: Row[];
}

export function MessageHeatmap({ data }: Props) {
  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        No data yet
      </div>
    );
  }

  const maxCount = Math.max(...data.map((d) => d.count), 1);
  const countMap = new Map(data.map((d) => [d.date, d.count]));

  // Build weeks for last ~5 months (like GitHub)
  const today = new Date();
  const weeks: { date: Date; count: number }[][] = [];

  // Start from ~20 weeks ago, aligned to Sunday
  const start = new Date(today);
  start.setDate(start.getDate() - 20 * 7 - start.getDay());

  let week: { date: Date; count: number }[] = [];
  const cursor = new Date(start);

  while (cursor <= today) {
    const dateStr = cursor.toISOString().slice(0, 10);
    week.push({ date: new Date(cursor), count: countMap.get(dateStr) ?? 0 });
    if (week.length === 7) {
      weeks.push(week);
      week = [];
    }
    cursor.setDate(cursor.getDate() + 1);
  }
  if (week.length > 0) weeks.push(week);

  const dayLabels = ["", "Mon", "", "Wed", "", "Fri", ""];

  return (
    <div className="flex h-full flex-col justify-center overflow-x-auto">
      <div className="flex gap-0.5">
        {/* Day labels */}
        <div className="flex flex-col gap-0.5 pr-1">
          {dayLabels.map((d, i) => (
            <div
              key={i}
              className="h-[13px] text-[9px] text-muted-foreground leading-[13px]"
            >
              {d}
            </div>
          ))}
        </div>
        {/* Week columns */}
        {weeks.map((w, wi) => (
          <div key={wi} className="flex flex-col gap-0.5">
            {w.map((cell, ci) => {
              const level =
                cell.count === 0 ? 0 : Math.ceil((cell.count / maxCount) * 4);
              return (
                <div
                  key={ci}
                  className="size-[13px] rounded-[2px]"
                  style={{
                    background:
                      level === 0
                        ? "var(--color-muted)"
                        : "var(--color-chart-1)",
                    opacity: level === 0 ? 1 : level * 0.25,
                  }}
                  title={`${cell.date.toISOString().slice(0, 10)}: ${cell.count} messages`}
                />
              );
            })}
          </div>
        ))}
      </div>
      {/* Intensity legend */}
      <div className="flex items-center gap-1 mt-2 text-[9px] text-muted-foreground">
        <span>Less</span>
        {[0, 1, 2, 3, 4].map((l) => (
          <div
            key={l}
            className="size-[10px] rounded-[2px]"
            style={{
              background:
                l === 0
                  ? "var(--color-muted)"
                  : "var(--color-chart-1)",
              opacity: l === 0 ? 1 : l * 0.25,
            }}
          />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}

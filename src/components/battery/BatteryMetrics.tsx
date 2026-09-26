import { MetricCard } from "@/components/battery/MetricCard";
import { Progress } from "@/components/ui/switch";
import { fmtNumber } from "@/lib/format";
import type { BatterySnapshot } from "@/lib/types";

interface BatteryMetricsProps {
  snapshot: BatterySnapshot;
}

export function BatteryMetrics({ snapshot }: BatteryMetricsProps) {
  const remaining = snapshot.remaining_capacity;
  const rated = snapshot.rated_capacity || 0;
  const filled = rated > 0 ? Math.max(0, Math.min(100, (remaining / rated) * 100)) : 0;

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
      <MetricCard label="Cycles" value={fmtNumber(snapshot.cycles, 0)} />
      <MetricCard label="Produced" value={snapshot.production_date || "–"} />
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Remaining / Rated
        </p>
        <p className="mt-1 text-xl font-semibold tabular-nums">
          {fmtNumber(remaining)}
          <span className="ml-1 text-sm font-normal text-muted-foreground">/ {fmtNumber(rated)} Ah</span>
        </p>
        <Progress value={filled} className="mt-2" />
      </div>
    </div>
  );
}
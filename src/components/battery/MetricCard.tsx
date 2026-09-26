import { cn } from "@/lib/utils";
import type { ReactNode } from "react";

interface MetricCardProps {
  label: string;
  value: string;
  unit?: string;
  sub?: ReactNode;
  className?: string;
  accent?: string;
}

export function MetricCard({ label, value, unit, sub, className, accent }: MetricCardProps) {
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className={cn("mt-1 text-xl font-semibold tabular-nums", accent, className)}>
        {value}
        {unit ? <span className="ml-1 text-sm font-normal text-muted-foreground">{unit}</span> : null}
      </p>
      {sub ? <p className="mt-1 text-xs text-muted-foreground">{sub}</p> : null}
    </div>
  );
}
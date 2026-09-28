import { cn } from "@/lib/utils";
import { cellDeltaSeverity } from "@/lib/batteryStatus";
import { fmtNumber } from "@/lib/format";

const IMBALANCE_MARGIN = 0.03;

interface CellVoltageBarsProps {
  voltages: number[];
}

export function CellVoltageBars({ voltages }: CellVoltageBarsProps) {
  if (voltages.length === 0) {
    return <p className="text-sm text-muted-foreground">No cell voltage data yet.</p>;
  }

  const min = Math.min(...voltages);
  const max = Math.max(...voltages);
  const delta = max - min;
  const avg = voltages.reduce((sum, value) => sum + value, 0) / voltages.length;
  const span = Math.max(delta, 0.001);

  const isImbalanced = (value: number) => Math.abs(value - avg) > IMBALANCE_MARGIN && delta >= 0.05;

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <div className="rounded-md border border-border bg-card p-3 text-center">
          <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Min</p>
          <p className="text-sm font-semibold tabular-nums">{fmtNumber(min, 3)} V</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3 text-center">
          <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Max</p>
          <p className="text-sm font-semibold tabular-nums">{fmtNumber(max, 3)} V</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3 text-center">
          <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Delta</p>
          <p className={cn("text-sm font-semibold tabular-nums", delta >= 0.1 ? "text-destructive" : "")}>
            {fmtNumber(delta, 3)} V
          </p>
        </div>
        <div className="rounded-md border border-border bg-card p-3 text-center">
          <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Avg</p>
          <p className="text-sm font-semibold tabular-nums">{fmtNumber(avg, 3)} V</p>
        </div>
      </div>

      <CellSummary severity={cellDeltaSeverity(delta)} />

      <div className="flex flex-wrap gap-2">
        {voltages.map((voltage, index) => {
          const height = Math.round(((voltage - min) / span) * 55) + 25;
          const isMin = voltage === min;
          const isMax = voltage === max;
          const imbalanced = isImbalanced(voltage);
          return (
            <div key={index} className="flex flex-col items-center gap-1">
              <p className="text-micro tabular-nums text-muted-foreground">{fmtNumber(voltage, 3)}</p>
              <div className="flex h-20 items-end rounded-sm border border-border bg-muted/40 p-0.5">
                <div
                  className={cn(
                    "w-4 rounded-sm transition-[height] duration-300",
                    imbalanced
                      ? "bg-amber-500"
                      : isMax
                        ? "bg-primary/70"
                        : isMin
                          ? "bg-sky-500/70"
                          : "bg-primary/60",
                  )}
                  style={{ height: `${height}%` }}
                  title={`Cell ${index + 1}: ${voltage.toFixed(3)} V`}
                />
              </div>
              <p className="text-micro text-muted-foreground">C{index + 1}</p>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function CellSummary({ severity }: { severity: "success" | "warning" | "destructive" }) {
  const text =
    severity === "success"
      ? "Cells are well balanced."
      : severity === "warning"
        ? "Cells show mild imbalance."
        : "Cell imbalance is significant.";
  return (
    <p
      className={cn(
        "text-xs font-medium",
        severity === "success"
          ? "text-emerald-600 dark:text-emerald-400"
          : severity === "warning"
            ? "text-amber-600 dark:text-amber-400"
            : "text-destructive",
      )}
    >
      {text}
    </p>
  );
}
import { cellDeltaSeverity } from "@/lib/batteryStatus";
import { fmtNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

export function CellPackGrid({ voltages }: { voltages: number[] }) {
  if (voltages.length === 0) {
    return <p className="text-sm text-muted-foreground">No cell voltage data yet.</p>;
  }

  const min = Math.min(...voltages);
  const max = Math.max(...voltages);
  const delta = max - min;
  const avg = voltages.reduce((sum, value) => sum + value, 0) / voltages.length;
  const severity = cellDeltaSeverity(delta);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-label tabular-nums text-muted-foreground">
        <span>min <span className="font-semibold text-foreground">{fmtNumber(min, 3)} V</span></span>
        <span>max <span className="font-semibold text-foreground">{fmtNumber(max, 3)} V</span></span>
        <span>
          delta{" "}
          <span className={cn("font-semibold", delta >= 0.1 ? "text-destructive" : "text-foreground")}>
            {fmtNumber(delta, 3)} V
          </span>
        </span>
        <span>avg <span className="font-semibold text-foreground">{fmtNumber(avg, 3)} V</span></span>
        <span
          className={cn(
            "text-xs font-medium",
            severity === "success"
              ? "text-emerald-600 dark:text-emerald-400"
              : severity === "warning"
                ? "text-amber-600 dark:text-amber-400"
                : "text-destructive",
          )}
        >
          {severity === "success" ? "well balanced" : severity === "warning" ? "mild imbalance" : "significant imbalance"}
        </span>
      </div>

      <div className="grid grid-cols-4 gap-1.5 sm:grid-cols-6 lg:grid-cols-8">
        {voltages.map((voltage, index) => {
          const t = delta > 0.0005 ? (voltage - min) / delta : 0.5;
          const isMin = voltage === min && delta > 0.0005;
          const isMax = voltage === max && delta > 0.0005;
          return (
            <div
              key={index}
              title={`Cell ${index + 1}: ${voltage.toFixed(3)} V`}
              className={cn(
                "rounded-md border px-2 py-1.5 text-center",
                isMin
                  ? "border-sky-500/50 bg-sky-500/10"
                  : isMax
                    ? "border-primary/50 bg-primary/10"
                    : "border-border bg-muted",
              )}
            >
              <p className="text-micro font-medium text-muted-foreground">C{index + 1}</p>
              <p className="text-label font-semibold tabular-nums">{fmtNumber(voltage, 3)}</p>
              <div className="mt-1 h-1 overflow-hidden rounded-full bg-border">
                <div
                  className={cn("h-full rounded-full", delta >= 0.1 ? "bg-destructive" : "bg-primary")}
                  style={{ width: `${Math.round(t * 100)}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

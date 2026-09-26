import { cn } from "@/lib/utils";
import { temperatureSeverity } from "@/lib/batteryStatus";
import { fmtNumber } from "@/lib/format";

interface TemperatureGaugesProps {
  temperatures: number[];
}

export function TemperatureGauges({ temperatures }: TemperatureGaugesProps) {
  if (temperatures.length === 0) {
    return <p className="text-sm text-muted-foreground">No temperature data yet.</p>;
  }
  return (
    <div className="flex flex-wrap gap-2">
      {temperatures.map((temperature, index) => {
        const severity = temperatureSeverity(temperature);
        return (
          <span
            key={index}
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm tabular-nums",
              severity === "success"
                ? "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                : severity === "warning"
                  ? "border-transparent bg-amber-500/15 text-amber-600 dark:text-amber-400"
                  : "border-transparent bg-destructive text-destructive-foreground",
            )}
          >
            <span
              className={cn(
                "size-1.5 rounded-full",
                severity === "success"
                  ? "bg-emerald-500"
                  : severity === "warning"
                    ? "bg-amber-500"
                    : "bg-destructive",
              )}
            />
            T{index + 1}: {fmtNumber(temperature, 1)}°C
          </span>
        );
      })}
    </div>
  );
}
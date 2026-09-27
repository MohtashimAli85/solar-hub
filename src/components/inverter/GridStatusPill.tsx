import type { GridBasis, GridStatus } from "@/lib/types";
import { fmtNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

const BASIS: Record<GridBasis, string> = {
  battery_draw_in_solar_mode:
    "The battery is carrying the house while the inverter is in Solar mode (panels, then grid, then battery), which only happens when the grid is gone.",
  ac_input: "Read from the inverter's AC input voltage.",
  load_without_battery: "The house load is covered without the battery or enough PV, so mains must be supplying it.",
  unknown: "Not enough data yet — no AC reading and no battery current.",
};

export function GridStatusPill({ grid, stale = false, className }: { grid: GridStatus | null | undefined; stale?: boolean; className?: string }) {
  const on = grid?.on ?? null;
  const details = [
    grid?.voltage != null && on ? `${fmtNumber(grid.voltage, 0)} V` : null,
    grid?.power_w != null && on ? `${fmtNumber(Math.round(grid.power_w), 0)} W` : null,
  ].filter(Boolean);
  const label =
    on === true
      ? ["Grid on", ...details].join(" · ")
      : on === false
        ? grid?.basis === "battery_draw_in_solar_mode"
          ? "Grid off · on battery"
          : "Grid off"
        : "Grid unknown";

  return (
    <span
      title={`${BASIS[grid?.basis ?? "unknown"]}${stale ? " Power figures are from the last reading (up to 2 minutes old)." : ""}`}
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-0.5 text-xs font-medium tabular-nums",
        on === true && "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        on === false && "border-amber-500/40 bg-amber-500/15 text-amber-700 dark:text-amber-300",
        on == null && "border-border bg-muted/40 text-muted-foreground",
        className,
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          on === true ? "bg-emerald-500" : on === false ? "bg-amber-500" : "bg-muted-foreground",
        )}
        aria-hidden
      />
      {label}
      {stale ? <span className="font-normal opacity-70">· stale</span> : null}
    </span>
  );
}

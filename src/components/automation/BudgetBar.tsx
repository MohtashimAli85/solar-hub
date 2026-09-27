import { cn } from "@/lib/utils";

interface BudgetBarProps {
  soc: number;
  floor: number;
  reserve?: number | null;
  /** A second marker, e.g. the SOC expected at sunset during the day. */
  projected?: number | null;
  projectedLabel?: string;
}

const clamp = (value: number) => Math.min(Math.max(value, 0), 100);

/**
 * The battery drawn as tonight's budget: the hard floor, the backup the
 * agent is keeping, and the slice it plans to spend.
 */
export function BudgetBar({ soc, floor, reserve, projected, projectedLabel }: BudgetBarProps) {
  const level = clamp(soc);
  const floorAt = clamp(floor);
  const reserveAt = reserve != null ? clamp(Math.max(reserve, floor)) : null;
  const spending = reserveAt != null && level > reserveAt;

  const segments = [
    { key: "floor", from: 0, to: Math.min(floorAt, level), className: "viz-hatch bg-muted" },
    reserveAt != null
      ? { key: "backup", from: floorAt, to: Math.min(reserveAt, level), className: "bg-[var(--viz-context-strong)]" }
      : null,
    {
      key: "spend",
      from: reserveAt ?? floorAt,
      to: level,
      className: spending || reserveAt == null ? "bg-[var(--viz-accent)]" : "bg-[var(--viz-context-strong)]",
    },
  ].filter((segment): segment is { key: string; from: number; to: number; className: string } =>
    segment != null && segment.to > segment.from,
  );

  const summary =
    reserveAt != null
      ? `Battery ${Math.round(level)}%. Floor ${Math.round(floorAt)}%, backup kept down to ${Math.round(reserveAt)}%, ${spending ? `${Math.round(level - reserveAt)} points available to spend tonight` : "nothing left to spend tonight"}.`
      : `Battery ${Math.round(level)}%, floor ${Math.round(floorAt)}%.`;

  return (
    <figure className="space-y-2" aria-label={summary}>
      <div className="relative h-7 overflow-hidden rounded-md border border-border bg-muted/30">
        {segments.map((segment) => (
          <div
            key={segment.key}
            className={cn("absolute inset-y-0 border-r-2 border-card last:border-r-0", segment.className)}
            style={{ left: `${segment.from}%`, width: `${segment.to - segment.from}%` }}
          />
        ))}
        {projected != null ? (
          <div
            className="absolute inset-y-0 w-0.5 bg-foreground/70"
            style={{ left: `calc(${clamp(projected)}% - 1px)` }}
            aria-hidden
          />
        ) : null}
        <span className="absolute inset-y-0 right-2 flex items-center text-xs font-semibold tabular-nums text-foreground">
          {Math.round(level)}%
        </span>
      </div>
      <figcaption className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
        <Legend swatch="viz-hatch bg-muted" label={`Floor ${Math.round(floorAt)}%`} />
        {reserveAt != null ? (
          <>
            <Legend swatch="bg-[var(--viz-context-strong)]" label={`Backup to ${Math.round(reserveAt)}%`} />
            <Legend
              swatch="bg-[var(--viz-accent)]"
              label={spending ? `Spending ${Math.round(level)}% → ${Math.round(reserveAt)}%` : "Nothing to spend"}
            />
          </>
        ) : (
          <Legend swatch="bg-[var(--viz-accent)]" label={`Available above floor ${Math.max(Math.round(level - floorAt), 0)} pts`} />
        )}
        {projected != null && projectedLabel ? <Legend swatch="bg-foreground/70 w-0.5" label={projectedLabel} /> : null}
      </figcaption>
    </figure>
  );
}

function Legend({ swatch, label }: { swatch: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className={cn("inline-block h-2.5 w-3 rounded-[2px]", swatch)} aria-hidden />
      {label}
    </span>
  );
}

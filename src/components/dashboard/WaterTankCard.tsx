import { clock } from "@/components/automation/shared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { WaterPhase, WaterStatus } from "@/lib/types";
import { Check, Droplets } from "lucide-react";

const PHASES: Record<WaterPhase, { label: string; variant: "secondary" | "info" | "success" | "warning" }> = {
  off: { label: "Off", variant: "secondary" },
  waiting: { label: "Waiting", variant: "secondary" },
  ready: { label: "Turn on motor", variant: "info" },
  done: { label: "Done", variant: "success" },
  missed: { label: "Not filled", variant: "warning" },
};

function headline(water: WaterStatus): string {
  switch (water.phase) {
    case "off":
      return "Reminders are off";
    case "done":
      return "Tank filled today";
    case "missed":
      return "Tank not filled today";
    case "ready":
      return "Good solar: turn on the motor";
    default:
      return "Waiting for good solar";
  }
}

interface WaterTankCardProps {
  water: WaterStatus | undefined;
  pending: boolean;
  onDone: () => void;
}

export function WaterTankCard({ water, pending, onDone }: WaterTankCardProps) {
  if (!water) {
    return <section className="min-h-32 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }
  const phase = PHASES[water.phase];
  const canMarkDone = water.phase === "waiting" || water.phase === "ready" || water.phase === "missed";

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:p-5">
      <div className="flex items-center justify-between gap-2">
        <h3 className="inline-flex items-center gap-2 text-label font-semibold uppercase tracking-wide text-muted-foreground">
          <Droplets className="size-4" aria-hidden /> Water tank
        </h3>
        <Badge variant={phase.variant}>{phase.label}</Badge>
      </div>
      <p className="text-lg font-semibold leading-tight">{headline(water)}</p>
      {water.note && water.phase !== "done" ? (
        <p className="text-sm leading-snug text-muted-foreground">{water.note}</p>
      ) : null}
      <div className="mt-auto flex flex-wrap items-center justify-between gap-2 pt-1">
        <span className="text-xs tabular-nums text-muted-foreground">
          {water.last_reminded_at ? `Last reminder ${clock(water.last_reminded_at)}` : ""}
        </span>
        {canMarkDone ? (
          <Button size="sm" onClick={onDone} disabled={pending}>
            <Check /> {pending ? "Saving…" : "Done"}
          </Button>
        ) : null}
      </div>
    </section>
  );
}

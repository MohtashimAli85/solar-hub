import type { ReactNode } from "react";
import { clock, modelLabel } from "@/components/automation/shared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { EvCharge, EvStatus, GeyserStatus, LoadPhase, LoadsStatus, SocBasis, WaterPhase, WaterStatus } from "@/lib/types";
import { Bike, Check, Droplets, Flame, RefreshCw, Sun } from "lucide-react";

type BadgeVariant = "secondary" | "info" | "success" | "warning";

const WATER_PHASES: Record<WaterPhase, { label: string; variant: BadgeVariant }> = {
  off: { label: "Off", variant: "secondary" },
  waiting: { label: "Waiting", variant: "secondary" },
  ready: { label: "Turn on motor", variant: "info" },
  done: { label: "Done", variant: "success" },
  missed: { label: "Not filled", variant: "warning" },
};

const LOAD_PHASES: Record<LoadPhase, { label: string; variant: BadgeVariant }> = {
  off: { label: "Off", variant: "secondary" },
  waiting: { label: "Waiting", variant: "secondary" },
  ready: { label: "Turn on", variant: "info" },
  running: { label: "Running", variant: "success" },
  stop: { label: "Turn off", variant: "warning" },
  done: { label: "Done", variant: "success" },
  skipped: { label: "Not today", variant: "secondary" },
};

const SOC_BASIS: Record<SocBasis, string> = {
  read: "read",
  live: "read live",
  estimated: "estimated",
  full: "charger finished",
};

const EV_CHARGE: Record<EvCharge, string> = {
  yes: "Charge today",
  if_spare: "Charge if the sun is spare",
  no: "No charge needed today",
};

function waterHeadline(water: WaterStatus): string {
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

function Row({
  icon,
  title,
  badge,
  headline,
  children,
  actions,
}: {
  icon: ReactNode;
  title: string;
  badge: { label: string; variant: BadgeVariant };
  headline: string;
  children?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="inline-flex items-center gap-2 text-sm font-semibold">
          {icon} {title}
        </h4>
        <Badge variant={badge.variant}>{badge.label}</Badge>
      </div>
      <p className="text-sm leading-snug">{headline}</p>
      {children}
      {actions ? <div className="mt-auto flex flex-wrap items-center justify-end gap-2 pt-1">{actions}</div> : null}
    </div>
  );
}

function Detail({ children }: { children: ReactNode }) {
  return <p className="text-xs leading-snug text-muted-foreground tabular-nums">{children}</p>;
}

function WaterRow({ water, runningSince, pending, onDone }: { water: WaterStatus; runningSince: string | null; pending: boolean; onDone: () => void }) {
  const canMarkDone = water.phase === "waiting" || water.phase === "ready" || water.phase === "missed";
  return (
    <Row
      icon={<Droplets className="size-4" aria-hidden />}
      title="Water tank"
      badge={runningSince ? LOAD_PHASES.running : WATER_PHASES[water.phase]}
      headline={runningSince ? `Motor running since ${clock(runningSince)}` : waterHeadline(water)}
      actions={
        canMarkDone ? (
          <Button size="sm" onClick={onDone} disabled={pending}>
            <Check /> {pending ? "Saving…" : "Done"}
          </Button>
        ) : null
      }
    >
      {water.note && water.phase !== "done" ? <Detail>{water.note}</Detail> : null}
      {water.last_reminded_at ? <Detail>Last reminder {clock(water.last_reminded_at)}</Detail> : null}
    </Row>
  );
}

function GeyserRow({ geyser, pending, onDone }: { geyser: GeyserStatus; pending: boolean; onDone: () => void }) {
  const canMarkDone = geyser.phase !== "off" && geyser.phase !== "done";
  return (
    <Row
      icon={<Flame className="size-4" aria-hidden />}
      title={geyser.auto ? "Geyser · ESP32" : "Geyser"}
      badge={LOAD_PHASES[geyser.phase]}
      headline={geyser.note ?? "–"}
      actions={
        canMarkDone ? (
          <Button size="sm" variant="outline" onClick={onDone} disabled={pending}>
            <Check /> {pending ? "Saving…" : "Hot enough"}
          </Button>
        ) : null
      }
    >
      {geyser.running_w != null && geyser.running_since ? (
        <Detail>
          Heating at {geyser.running_w.toFixed(0)} W since {clock(geyser.running_since)}
        </Detail>
      ) : null}
      <Detail>
        {geyser.delivered_kwh.toFixed(1)} of about {geyser.need_kwh.toFixed(1)} kWh today
        {geyser.heated_at ? ` · hot at ${clock(geyser.heated_at)}` : ""}
      </Detail>
    </Row>
  );
}

function ScooterRow({
  ev,
  pending,
  skipPending,
  onRead,
  onSkip,
}: {
  ev: EvStatus;
  pending: boolean;
  skipPending: boolean;
  onRead: () => void;
  onSkip: () => void;
}) {
  const busy = pending || ev.reading;
  const canSkip = ev.phase === "ready" || ev.phase === "waiting" || ev.phase === "stop";
  const soc =
    ev.soc != null
      ? `${ev.soc.toFixed(0)}%${ev.soc_basis ? ` · ${SOC_BASIS[ev.soc_basis]}` : ""}${ev.soc_at ? ` ${clock(ev.soc_at)}` : ""}`
      : "Charge not known yet";
  return (
    <Row
      icon={<Bike className="size-4" aria-hidden />}
      title="Scooter"
      badge={LOAD_PHASES[ev.phase]}
      headline={ev.note ?? "–"}
      actions={
        ev.phase !== "off" ? (
          <>
            {canSkip ? (
              <Button size="sm" variant="ghost" onClick={onSkip} disabled={skipPending}>
                Not today
              </Button>
            ) : null}
            <Button size="sm" variant="outline" onClick={onRead} disabled={busy}>
              <RefreshCw className={busy ? "animate-spin motion-reduce:animate-none" : undefined} /> {busy ? "Reading…" : "Read now"}
            </Button>
          </>
        ) : null
      }
    >
      <Detail>
        {soc}
        {ev.hours_to_full != null && ev.hours_to_full > 0.05 ? ` · ${ev.hours_to_full.toFixed(1)} h to full` : ""}
      </Detail>
      {ev.decision ? (
        <Detail>
          {EV_CHARGE[ev.decision]}
          {ev.decision_reason ? ` — ${ev.decision_reason}` : " (the app's own rule)"}
          {ev.decision_model ? ` · ${modelLabel(ev.decision_model)}` : ""}
        </Detail>
      ) : null}
      {ev.read_error ? <Detail>Last read failed: {ev.read_error}</Detail> : null}
    </Row>
  );
}

interface SolarLoadsCardProps {
  water: WaterStatus | undefined;
  loads: LoadsStatus | undefined;
  waterPending: boolean;
  geyserPending: boolean;
  scooterPending: boolean;
  skipPending: boolean;
  onWaterDone: () => void;
  onGeyserDone: () => void;
  onReadScooter: () => void;
  onSkipScooter: () => void;
}

export function SolarLoadsCard({
  water,
  loads,
  waterPending,
  geyserPending,
  scooterPending,
  skipPending,
  onWaterDone,
  onGeyserDone,
  onReadScooter,
  onSkipScooter,
}: SolarLoadsCardProps) {
  if (!water || !loads) {
    return <section className="min-h-32 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }
  const outlook =
    loads.spare_kwh != null && loads.projected_sunset_soc != null && loads.sunset_target_soc != null
      ? `About ${loads.spare_kwh.toFixed(1)} kWh still spare today · battery ${loads.projected_sunset_soc.toFixed(0)}% by sunset (tonight needs ${loads.sunset_target_soc.toFixed(0)}%)`
      : null;

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="inline-flex items-center gap-2 text-label font-semibold uppercase tracking-wide text-muted-foreground">
          <Sun className="size-4" aria-hidden /> Solar loads
        </h3>
        {outlook ? <span className="text-xs tabular-nums text-muted-foreground">{outlook}</span> : null}
      </div>
      <div className="grid gap-3 md:grid-cols-3">
        <WaterRow water={water} runningSince={loads.water_running_since} pending={waterPending} onDone={onWaterDone} />
        <GeyserRow geyser={loads.geyser} pending={geyserPending} onDone={onGeyserDone} />
        <ScooterRow ev={loads.ev} pending={scooterPending} skipPending={skipPending} onRead={onReadScooter} onSkip={onSkipScooter} />
      </div>
    </section>
  );
}

import { useEffect, useState } from "react";
import { GridStatusPill } from "@/components/inverter/GridStatusPill";
import { PvBreakdown } from "@/components/inverter/PvBreakdown";
import { Skeleton } from "@/components/ui/switch";
import { batteryEta, describeEta, EMPTY_TARGET_PERCENT } from "@/lib/batteryEta";
import { chargeState } from "@/lib/batteryStatus";
import { fieldPowerWatts, fieldValue, flowPowerWatts, fmtDuration, fmtNumber } from "@/lib/format";
import type { BatterySnapshot, ConnectionStatus, InverterSnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

interface CommandHeroProps {
  battery: BatterySnapshot | null;
  connection: ConnectionStatus | null;
  inverter: InverterSnapshot | null;
  inverterUpdatedAt: number | null;
  sunset: string | null;
  batteryLoading: boolean;
  inverterLoading: boolean;
}

function wattsParts(watts: number): { value: string; unit: string } {
  const abs = Math.abs(watts);
  return abs >= 1000 ? { value: (abs / 1000).toFixed(2), unit: "kW" } : { value: String(Math.round(abs)), unit: "W" };
}

function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

function ageText(updatedAt: number | null, now: number): string | null {
  if (updatedAt == null) return null;
  const seconds = Math.max(0, Math.round((now - updatedAt) / 1000));
  if (seconds < 10) return "updated just now";
  if (seconds < 60) return `updated ${seconds} s ago`;
  return `updated ${fmtDuration(seconds / 60)} ago`;
}

export function CommandHero({ battery, connection, inverter, inverterUpdatedAt, sunset, batteryLoading, inverterLoading }: CommandHeroProps) {
  const now = useNow(10_000);
  const fields = inverter?.fields ?? {};
  const bmsOk = connection?.connected ?? battery != null;
  const cloudOk = inverter != null;
  const age = ageText(inverterUpdatedAt, now);
  const stale = inverterUpdatedAt != null && now - inverterUpdatedAt > 120_000;

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-4 sm:px-5 py-3 text-xs">
        <span className="font-semibold uppercase tracking-wide text-muted-foreground">Live plant</span>
        <span className="ml-auto flex flex-wrap items-center gap-3 text-muted-foreground">
          <StatusDot ok={bmsOk} label={bmsOk ? "Battery connected" : "Battery offline"} title={connection?.device_name ?? undefined} />
          <StatusDot ok={cloudOk && !stale} warn={cloudOk && stale} label={cloudOk ? `Inverter cloud${age ? ` · ${age}` : ""}` : "Inverter cloud offline"} />
        </span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 sm:divide-x lg:grid-cols-4">
        <BatteryColumn battery={battery} loading={batteryLoading} />
        <RateColumn battery={battery} loading={batteryLoading} now={now} sunset={sunset} />
        <SolarColumn inverter={inverter} loading={inverterLoading} fields={fields} />
        <HouseColumn inverter={inverter} loading={inverterLoading} fields={fields} />
      </div>
    </section>
  );
}

function Column({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("border-t px-4 sm:px-5 py-4 first:border-t-0 sm:[&:nth-child(2)]:border-t-0 lg:border-t-0", className)}>
      <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      {children}
    </div>
  );
}

function BigNumber({ value, unit, className }: { value: string; unit: string; className?: string }) {
  return (
    <p className={cn("mt-1 text-3xl font-semibold leading-none tabular-nums sm:text-4xl xl:text-5xl", className)}>
      {value}
      <span className="ml-1 text-lg font-medium text-muted-foreground">{unit}</span>
    </p>
  );
}

function BatteryColumn({ battery, loading }: { battery: BatterySnapshot | null; loading: boolean }) {
  const soc = battery?.soc ?? null;
  return (
    <Column label="Battery">
      {loading ? (
        <Skeleton className="mt-2 h-12 w-32" />
      ) : soc == null || !battery ? (
        <p className="mt-1 text-sm text-muted-foreground">Connect the battery (BMS) on the Battery page.</p>
      ) : (
        <>
          <BigNumber value={fmtNumber(soc, 0)} unit="%" className={cn(soc < 20 && "text-destructive")} />
          <div
            className="relative mt-3 h-2 overflow-hidden rounded-full bg-muted"
            role="progressbar"
            aria-label="Battery charge"
            aria-valuenow={Math.round(soc)}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div
              className={cn("h-full rounded-full", soc < 20 ? "bg-destructive" : "bg-primary")}
              style={{ width: `${Math.max(0, Math.min(100, soc))}%` }}
            />
            <span
              className="absolute inset-y-0 w-0.5 bg-foreground/50"
              style={{ left: `${EMPTY_TARGET_PERCENT}%` }}
              title={`${EMPTY_TARGET_PERCENT}% — where the time estimate counts down to`}
              aria-hidden
            />
          </div>
          <p className="mt-2 text-xs tabular-nums text-muted-foreground">
            {fmtNumber(battery.remaining_capacity, 1)} of {fmtNumber(battery.rated_capacity, 0)} Ah · {fmtNumber(battery.voltage, 1)} V
          </p>
        </>
      )}
    </Column>
  );
}

function RateColumn({ battery, loading, now, sunset }: { battery: BatterySnapshot | null; loading: boolean; now: number; sunset: string | null }) {
  if (loading) {
    return (
      <Column label="Charge / discharge">
        <Skeleton className="mt-2 h-12 w-40" />
      </Column>
    );
  }
  if (!battery) {
    return (
      <Column label="Charge / discharge">
        <p className="mt-1 text-sm text-muted-foreground">No battery readings yet.</p>
      </Column>
    );
  }
  const state = chargeState(battery.current);
  const watts = battery.voltage * battery.current;
  const sunsetMs = sunset ? new Date(sunset).getTime() : NaN;
  const eta = describeEta(batteryEta(battery, now), now, Number.isFinite(sunsetMs) && sunsetMs > now ? sunsetMs : null);
  const moving = state === "charging" || state === "discharging";
  const power = wattsParts(watts);
  const amps = `${fmtNumber(Math.abs(battery.current), 1)} A`;
  const flow = state === "charging" ? `${amps} into the battery` : state === "discharging" ? `${amps} from the battery` : "no flow";

  return (
    <Column label={state === "charging" ? "Charging" : state === "discharging" ? "Discharging" : "Charge / discharge"}>
      <BigNumber value={moving ? power.value : "0"} unit={moving ? power.unit : "W"} className={cn(!moving && "text-muted-foreground")} />
      <p className="mt-2 text-xs tabular-nums text-muted-foreground">{flow}</p>
      <p className="mt-2 text-sm font-medium tabular-nums">{eta.headline}</p>
      {eta.detail ? <p className="text-xs text-muted-foreground">{eta.detail}</p> : null}
    </Column>
  );
}

function SolarColumn({ inverter, loading, fields }: { inverter: InverterSnapshot | null; loading: boolean; fields: InverterSnapshot["fields"] }) {
  const pv = inverter ? flowPowerWatts(inverter.pv_panel_flow) ?? fieldPowerWatts(fields, ["pvInputPower", "generationPower", "pvPower"]) : null;
  return (
    <Column label="Solar">
      {loading ? (
        <Skeleton className="mt-2 h-12 w-32" />
      ) : !inverter ? (
        <p className="mt-1 text-sm text-muted-foreground">Add Solar credentials in Settings.</p>
      ) : (
        <>
          <BigNumber value={pv == null ? "–" : fmtNumber(Math.round(pv), 0)} unit="W" />
          <p className="mt-2 text-xs text-muted-foreground">{pv != null && pv < 20 ? "No sun on the panels" : "Coming from the panels"}</p>
          <div className="mt-1">
            <PvBreakdown fields={fields} />
          </div>
        </>
      )}
    </Column>
  );
}

function HouseColumn({ inverter, loading, fields }: { inverter: InverterSnapshot | null; loading: boolean; fields: InverterSnapshot["fields"] }) {
  const load = inverter
    ? flowPowerWatts(inverter.load_flow) ?? fieldPowerWatts(fields, ["load_power", "loadPower", "outputActivePower", "acOutputActivePower"])
    : null;
  const loadPercent = fieldValue(fields, ["loadPercentage"]);
  return (
    <Column label="House">
      {loading ? (
        <Skeleton className="mt-2 h-12 w-32" />
      ) : !inverter ? (
        <p className="mt-1 text-sm text-muted-foreground">No inverter readings yet.</p>
      ) : (
        <>
          <BigNumber value={load == null ? "–" : fmtNumber(Math.round(load), 0)} unit="W" />
          <p className="mt-2 text-xs tabular-nums text-muted-foreground">
            {loadPercent != null ? `${fmtNumber(loadPercent, 0)}% of the inverter's capacity` : "Being used right now"}
          </p>
          <GridStatusPill grid={inverter.grid} stale={inverter.energy_flow_stale} className="mt-2" />
        </>
      )}
    </Column>
  );
}

function StatusDot({ ok, warn = false, label, title }: { ok: boolean; warn?: boolean; label: string; title?: string }) {
  return (
    <span className="inline-flex items-center gap-1.5" title={title}>
      <span className={cn("size-1.5 rounded-full", warn ? "bg-amber-500" : ok ? "bg-emerald-500" : "bg-destructive")} aria-hidden />
      {label}
    </span>
  );
}

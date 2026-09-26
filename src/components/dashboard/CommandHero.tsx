import { PvBreakdown } from "@/components/inverter/PvBreakdown";
import { Skeleton } from "@/components/ui/switch";
import { chargeState } from "@/lib/batteryStatus";
import { fieldPowerWatts, flowPowerWatts, fmtHours, fmtNumber, fmtSigned } from "@/lib/format";
import type { BatterySnapshot, ConnectionStatus, InverterSnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

interface CommandHeroProps {
  battery: BatterySnapshot | null;
  connection: ConnectionStatus | null;
  inverter: InverterSnapshot | null;
  batteryLoading: boolean;
  inverterLoading: boolean;
}

export function formatPackWatts(watts: number | null): string {
  if (watts == null || Number.isNaN(watts)) return "–";
  const abs = Math.abs(watts);
  if (abs >= 1000) {
    const kw = watts / 1000;
    const sign = kw > 0 ? "+" : kw < 0 ? "−" : "";
    return `${sign}${Math.abs(kw).toFixed(2)} kW`;
  }
  return `${fmtSigned(Math.round(watts), 0)} W`.replace("-", "−");
}

export function CommandHero({ battery, connection, inverter, batteryLoading, inverterLoading }: CommandHeroProps) {
  const fields = inverter?.fields ?? {};
  const bmsOk = connection?.connected ?? battery != null;
  const cloudOk = inverter != null;

  const soc = battery?.soc ?? null;
  const amps = battery?.current ?? null;
  const packWatts = battery != null ? battery.voltage * battery.current : null;
  const state = battery != null ? chargeState(battery.current) : null;

  const rateColor =
    state === "charging"
      ? "text-amber-600 dark:text-amber-400"
      : state === "discharging"
        ? "text-sky-600 dark:text-sky-400"
        : "text-muted-foreground";

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-5 py-3 text-xs">
        <span className="font-semibold uppercase tracking-wide text-muted-foreground">
          Live plant
        </span>
        <span className="ml-auto flex items-center gap-3 text-muted-foreground">
          <StatusDot ok={bmsOk} label={bmsOk ? `BMS ok${connection?.device_name ? ` · ${connection.device_name}` : ""}` : "BMS offline"} />
          <StatusDot ok={cloudOk} label={cloudOk ? "cloud ok" : "cloud offline"} />
        </span>
      </div>

      <div className="flex items-center gap-2 px-5 pt-4 text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
        <span className="text-amber-600 dark:text-amber-400">PV</span>
        <span aria-hidden className="h-px flex-1 bg-gradient-to-r from-amber-500/70 via-border to-primary/60" />
        <span aria-hidden>→</span>
        <span>Pack</span>
        <span aria-hidden className="h-px flex-1 bg-border" />
        <span aria-hidden>→</span>
        <span>Load</span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-3 sm:divide-x">
        <div className="px-5 py-4">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Pack</p>
          {batteryLoading ? (
            <Skeleton className="mt-2 h-12 w-32" />
          ) : soc == null ? (
            <p className="mt-1 text-sm text-muted-foreground">Connect a BMS on the Battery page.</p>
          ) : (
            <>
              <p className="text-5xl font-semibold tabular-nums leading-none">
                {fmtNumber(soc, 0)}
                <span className="ml-1 text-lg font-medium text-muted-foreground">%</span>
              </p>
              <div
                className="mt-3 h-2.5 overflow-hidden rounded-full bg-muted"
                role="progressbar"
                aria-valuenow={Math.round(soc)}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div
                  className={cn("h-full rounded-full", soc < 20 ? "bg-destructive" : "bg-primary")}
                  style={{ width: `${Math.max(0, Math.min(100, soc))}%` }}
                />
              </div>
              <p className="mt-2 text-xs tabular-nums text-muted-foreground">
                {battery ? `${fmtNumber(battery.voltage, 1)} V · ${fmtNumber(battery.remaining_capacity, 0)}/${fmtNumber(battery.rated_capacity, 0)} Ah` : "–"}
              </p>
            </>
          )}
        </div>

        <div className="border-t px-5 py-4 sm:border-t-0">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Charge / discharge</p>
          {batteryLoading ? (
            <Skeleton className="mt-2 h-12 w-40" />
          ) : packWatts == null ? (
            <p className="mt-1 text-sm text-muted-foreground">No pack telemetry.</p>
          ) : (
            <>
              <p className={cn("text-4xl font-semibold tabular-nums leading-tight sm:text-5xl sm:leading-none", rateColor)}>
                {`${fmtSigned(amps, 1)} A`.replace("-", "−")}
              </p>
              <p className="mt-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {state === "charging" ? <span className="text-amber-600 dark:text-amber-400">charging</span>
                  : state === "discharging" ? <span className="text-sky-600 dark:text-sky-400">discharging</span>
                    : "idle"}
                {packWatts != null ? <span className="ml-2 normal-case tabular-nums">{formatPackWatts(packWatts)}</span> : null}
              </p>
              {battery?.time_to_15_hours != null ? (
                <p className="mt-1 text-xs tabular-nums text-muted-foreground">
                  {fmtHours(battery.time_to_15_hours)} to 15% ·{" "}
                  {battery.current > 0.05 ? "while charging or idle" : "at current demand"}
                </p>
              ) : null}
            </>
          )}
        </div>

        <div className="border-t px-5 py-4 sm:border-t-0">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">PV making</p>
          {inverterLoading ? (
            <Skeleton className="mt-2 h-12 w-32" />
          ) : inverter == null ? (
            <p className="mt-1 text-sm text-muted-foreground">Add Solar credentials in Settings.</p>
          ) : (
            <PvWatts fields={fields} pvFlow={inverter?.pv_panel_flow} />
          )}
        </div>
      </div>
    </section>
  );
}

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className={cn("size-1.5 rounded-full", ok ? "bg-emerald-500" : "bg-destructive")} />
      {label}
    </span>
  );
}

function PvWatts({ fields, pvFlow }: { fields: InverterSnapshot["fields"]; pvFlow?: unknown }) {
  const pv = flowPowerWatts(pvFlow) ?? fieldPowerWatts(fields, ["pvInputPower", "generationPower", "pvPower"]);
  return (
    <>
      <p className="text-5xl font-semibold tabular-nums leading-none text-amber-600 dark:text-amber-400">
        {pv == null ? "–" : fmtNumber(Math.round(pv), 0)}
        <span className="ml-1 text-lg font-medium text-muted-foreground">W</span>
      </p>
      <div className="mt-2">
        <PvBreakdown fields={fields} />
      </div>
    </>
  );
}

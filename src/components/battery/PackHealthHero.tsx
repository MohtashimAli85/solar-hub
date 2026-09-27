import { Badge } from "@/components/ui/badge";
import { chargeState, parseFetStatus, parseProtectionFlags } from "@/lib/batteryStatus";
import { batteryEta, describeEta, etaDuration } from "@/lib/batteryEta";
import { fmtNumber, fmtSigned } from "@/lib/format";
import type { BatterySnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

export function PackHealthHero({ snapshot }: { snapshot: BatterySnapshot }) {
  const state = chargeState(snapshot.current);
  const packWatts = snapshot.voltage * snapshot.current;
  const fet = parseFetStatus(snapshot.fet_status);
  const protection = parseProtectionFlags(snapshot.protection_status);
  const eta = batteryEta(snapshot);
  const etaText = describeEta(eta);

  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
          Pack health
        </h3>
        <span className="ml-auto flex flex-wrap gap-1.5">
          <Badge variant={fet.charge ? "success" : "secondary"}>
            {fet.charge ? "CHG FET on" : "CHG FET off"}
          </Badge>
          <Badge variant={fet.discharge ? "success" : "secondary"}>
            {fet.discharge ? "DIS FET on" : "DIS FET off"}
          </Badge>
          {protection.length === 0 ? (
            <Badge variant="success">Protection normal</Badge>
          ) : (
            <Badge variant="destructive">
              {protection.length} fault{protection.length === 1 ? "" : "s"}: {protection.map((f) => f.label).join(", ")}
            </Badge>
          )}
        </span>
      </div>

      <div className="mt-3 grid gap-5 sm:grid-cols-5">
        <div className="sm:col-span-2">
          <p className="text-6xl font-semibold tabular-nums leading-none">
            {fmtNumber(snapshot.soc, 0)}
            <span className="ml-1 text-lg font-medium text-muted-foreground">%</span>
          </p>
          <div
            className="mt-3 h-3 overflow-hidden rounded-full bg-muted"
            role="progressbar"
            aria-valuenow={Math.round(snapshot.soc)}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div
              className={cn("h-full rounded-full", snapshot.soc < 20 ? "bg-destructive" : "bg-primary")}
              style={{ width: `${Math.max(0, Math.min(100, snapshot.soc))}%` }}
            />
          </div>
          <p className="mt-2 text-xs tabular-nums text-muted-foreground">
            {fmtNumber(snapshot.remaining_capacity, 0)}/{fmtNumber(snapshot.rated_capacity, 0)} Ah · {snapshot.cycles} cycles
          </p>
        </div>
        <div className="grid grid-cols-3 gap-3 sm:col-span-3">
          <HealthStat
            label="Pack power"
            value={`${fmtSigned(Math.round(packWatts), 0)} W`.replace("-", "−")}
            sub={state === "idle" ? "idle" : state === "charging" ? "charging" : "discharging"}
            accent={state === "charging" ? "text-amber-600 dark:text-amber-400" : state === "discharging" ? "text-sky-600 dark:text-sky-400" : undefined}
          />
          <HealthStat
            label="Current"
            value={`${fmtSigned(snapshot.current, 1)} A`}
            sub={`${fmtNumber(snapshot.voltage, 2)} V`}
          />
          <HealthStat
            label={eta.kind === "charging" || eta.kind === "full" ? "Time to full" : "Time to 15%"}
            value={etaDuration(eta)}
            sub={etaText.detail ?? undefined}
          />
        </div>
      </div>
    </section>
  );
}

function HealthStat({ label, value, sub, accent }: { label: string; value: string; sub?: string; accent?: string }) {
  return (
    <div className="rounded-lg border border-border bg-muted px-3 py-3">
      <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className={cn("mt-1 text-lg font-semibold tabular-nums", accent)}>{value}</p>
      {sub ? <p className="mt-0.5 text-[11px] text-muted-foreground">{sub}</p> : null}
    </div>
  );
}

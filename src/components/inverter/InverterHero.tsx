import { GridStatusPill } from "@/components/inverter/GridStatusPill";
import { PvBreakdown } from "@/components/inverter/PvBreakdown";
import { Badge } from "@/components/ui/badge";
import { chargeState } from "@/lib/batteryStatus";
import { fieldPowerWatts, fieldText, fieldValue, flowPowerWatts, fmtNumber } from "@/lib/format";
import type { BatterySnapshot, InverterSnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

interface InverterHeroProps {
  snapshot: InverterSnapshot;
  battery: BatterySnapshot | null;
  batteryDeviceName: string | null;
}

const STATUS_FIELDS: { label: string; keys: string[] }[] = [
  { label: "Grid", keys: ["mainsStatus"] },
  { label: "Load", keys: ["loadStatuss", "loadStatus"] },
  { label: "PV", keys: ["pvStatuss", "pvStatus"] },
];

function flowDirection(flow: unknown): number | null {
  if (!flow || typeof flow !== "object") return null;
  return ((flow as Record<string, unknown>).flowDirection as number) ?? null;
}

function directionText(direction: number | null, out: string, into: string): string | null {
  return direction === 1 ? out : direction === 2 ? into : null;
}

export function InverterHero({ snapshot, battery, batteryDeviceName }: InverterHeroProps) {
  const fields = snapshot.fields;
  const pvPower = flowPowerWatts(snapshot.pv_panel_flow) ?? fieldPowerWatts(fields, ["pvInputPower", "generationPower", "pvPower"]);
  const gridPower = flowPowerWatts(snapshot.grid_flow) ?? fieldValue(fields, ["gridPower"]);
  const loadPower =
    flowPowerWatts(snapshot.load_flow) ?? fieldPowerWatts(fields, ["load_power", "loadPower", "outputActivePower", "acOutputActivePower"]);
  const loadPercentage = fieldValue(fields, ["loadPercentage"]);
  const workingState = fieldText(fields, ["workingStates"]);
  const acInputVoltage = fieldValue(fields, ["acInputVoltage"]);
  const acInputFrequency = fieldValue(fields, ["acInputFrequency"]);
  const outputVoltage = fieldValue(fields, ["outputVoltage"]);
  const outputFrequency = fieldValue(fields, ["outputFrequency"]);
  const gridDirection = directionText(flowDirection(snapshot.grid_flow), "Exporting", "Importing");
  const statuses = STATUS_FIELDS.map((status) => ({ ...status, text: fieldText(fields, status.keys) })).filter((s) => s.text);

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b px-5 py-3 text-xs text-muted-foreground">
        <Badge variant={workingState ? "info" : "outline"}>{workingState ?? "Unknown state"}</Badge>
        <span className="tabular-nums">
          Grid in <span className="text-foreground">{fmtNumber(acInputVoltage, 0)} V · {fmtNumber(acInputFrequency, 1)} Hz</span>
        </span>
        <span className="tabular-nums">
          Output <span className="text-foreground">{fmtNumber(outputVoltage, 0)} V · {fmtNumber(outputFrequency, 1)} Hz</span>
        </span>
        {statuses.length ? (
          <span className="ml-auto flex flex-wrap gap-1.5">
            {statuses.map((status) => (
              <span key={status.label} className="rounded-full border border-border px-2 py-0.5">
                <span className="text-foreground">{status.label}</span> {status.text}
              </span>
            ))}
          </span>
        ) : null}
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-3 sm:divide-x">
        <FlowColumn label="Solar in" value={pvPower} tone="text-amber-600 dark:text-amber-400">
          <PvBreakdown fields={fields} />
        </FlowColumn>
        <FlowColumn label="House load" value={loadPower} className="border-t sm:border-t-0">
          <p className="text-xs tabular-nums text-muted-foreground">
            {loadPercentage != null ? `${fmtNumber(loadPercentage, 0)}% of the inverter's capacity` : "–"}
          </p>
        </FlowColumn>
        <FlowColumn label="Grid" value={gridPower} hint={gridDirection} className="border-t sm:border-t-0">
          <GridStatusPill grid={snapshot.grid} stale={snapshot.energy_flow_stale} />
        </FlowColumn>
      </div>

      <BatteryStrip battery={battery} deviceName={batteryDeviceName} />
    </section>
  );
}

function FlowColumn({
  label,
  value,
  hint,
  tone,
  className,
  children,
}: {
  label: string;
  value: number | null;
  hint?: string | null;
  tone?: string;
  className?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className={cn("space-y-2 px-5 py-4", className)}>
      <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
        {hint ? <span className="ml-auto normal-case tracking-normal">{hint}</span> : null}
      </div>
      <p className={cn("text-4xl font-semibold leading-none tabular-nums xl:text-5xl", tone)}>
        {fmtNumber(value != null ? Math.round(value) : null, 0)}
        <span className="ml-1 text-lg font-medium text-muted-foreground">W</span>
      </p>
      {children}
    </div>
  );
}

function BatteryStrip({ battery, deviceName }: { battery: BatterySnapshot | null; deviceName: string | null }) {
  if (!battery) {
    return (
      <p className="border-t bg-muted/30 px-5 py-2.5 text-xs text-muted-foreground">
        Battery: not connected — connect the BMS on the Battery page to see charge and current here.
      </p>
    );
  }
  const state = chargeState(battery.current);
  const flow =
    state === "idle"
      ? "idle"
      : `${fmtNumber(Math.abs(battery.current), 1)} A ${state === "charging" ? "charging" : "discharging"}`;
  return (
    <p className="border-t bg-muted/30 px-5 py-2.5 text-xs tabular-nums text-muted-foreground">
      Battery <span className="font-medium text-foreground">{fmtNumber(battery.soc, 0)}%</span> · {fmtNumber(battery.voltage, 1)} V ·{" "}
      <span className={cn(state === "charging" && "text-amber-600 dark:text-amber-400", state === "discharging" && "text-sky-600 dark:text-sky-400")}>
        {flow}
      </span>
      {deviceName ? ` · ${deviceName}` : ""}
    </p>
  );
}

import { PvBreakdown } from "@/components/inverter/PvBreakdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { fieldText, fieldPowerWatts, fieldValue, flowPowerWatts, fmtNumber, fmtSigned } from "@/lib/format";
import type { BatterySnapshot, InverterSnapshot } from "@/lib/types";
import { RefreshCw } from "lucide-react";

interface InverterHeroProps {
  snapshot: InverterSnapshot;
  battery: BatterySnapshot | null;
  batteryDeviceName: string | null;
  onRefresh: () => void;
  refreshing: boolean;
}

export function InverterHero({
  snapshot,
  battery,
  batteryDeviceName,
  onRefresh,
  refreshing,
}: InverterHeroProps) {
  const fields = snapshot.fields;

  const getFlowPower = (flow: unknown): number | null => flowPowerWatts(flow);

  const getFlowDirection = (flow: unknown): number | null => {
    if (!flow || typeof flow !== "object") return null;
    const f = flow as Record<string, unknown>;
    return (f.flowDirection as number) ?? null;
  };

  const pvPower = getFlowPower(snapshot.pv_panel_flow) ?? fieldPowerWatts(fields, ["pvInputPower", "generationPower", "pvPower"]);
  const gridPower = getFlowPower(snapshot.grid_flow) ?? fieldValue(fields, ["gridPower"]);
  const loadPower = getFlowPower(snapshot.load_flow) ?? fieldPowerWatts(fields, ["load_power", "loadPower", "outputActivePower", "acOutputActivePower"]);
  const loadPercentage = fieldValue(fields, ["loadPercentage"]);
  const workingState = fieldText(fields, ["workingStates"]);
  const acInputVoltage = fieldValue(fields, ["acInputVoltage"]);
  const acInputFrequency = fieldValue(fields, ["acInputFrequency"]);
  const outputVoltage = fieldValue(fields, ["outputVoltage"]);
  const outputFrequency = fieldValue(fields, ["outputFrequency"]);
  const pvDirection = getFlowDirection(snapshot.pv_panel_flow);
  const gridDirection = getFlowDirection(snapshot.grid_flow);
  const loadDirection = getFlowDirection(snapshot.load_flow);

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-5 py-3 text-xs text-muted-foreground">
        <Badge variant={workingState ? "info" : "outline"}>
          {workingState ?? "Unknown"}
        </Badge>
        <span className="tabular-nums">
          AC {fmtNumber(acInputVoltage, 1)} V · {fmtNumber(acInputFrequency, 1)} Hz
        </span>
        <span className="tabular-nums">
          Out {fmtNumber(outputVoltage, 1)} V · {fmtNumber(outputFrequency, 1)} Hz
        </span>
        <Button
          variant="ghost"
          size="icon"
          onClick={onRefresh}
          disabled={refreshing}
          aria-label="Refresh telemetry"
          className="ml-auto size-7"
        >
          <RefreshCw className={refreshing ? "animate-spin" : ""} />
        </Button>
      </div>

      <div className="flex items-center gap-2 px-5 pt-4 text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
        <span className="text-amber-600 dark:text-amber-400">PV</span>
        <span aria-hidden className="h-px flex-1 bg-gradient-to-r from-amber-500/70 via-border to-border" />
        <span aria-hidden>→</span>
        <span>Load</span>
        <span aria-hidden className="h-px flex-1 bg-border" />
        <span aria-hidden>→</span>
        <span>Grid</span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-3 sm:divide-x">
        <div className="px-5 py-4">
          <FlowLabel title="PV in" flowStatus={pvDirection === 1 ? "Outputting" : pvDirection === 2 ? "Inputting" : null} />
          <p className="text-5xl font-semibold tabular-nums leading-none text-amber-600 dark:text-amber-400">
            {fmtNumber(pvPower != null ? Math.round(pvPower) : null, 0)}
            <span className="ml-1 text-lg font-medium text-muted-foreground">W</span>
          </p>
          <div className="mt-2">
            <PvBreakdown fields={fields} />
          </div>
        </div>
        <div className="border-t px-5 py-4 sm:border-t-0">
          <FlowLabel title="Load" flowStatus={loadDirection === 1 ? "Outputting" : loadDirection === 2 ? "Inputting" : null} />
          <p className="text-5xl font-semibold tabular-nums leading-none">
            {fmtNumber(loadPower != null ? Math.round(loadPower) : null, 0)}
            <span className="ml-1 text-lg font-medium text-muted-foreground">W</span>
          </p>
          <p className="mt-2 text-xs tabular-nums text-muted-foreground">
            Load {fmtNumber(loadPercentage, 0)}%
          </p>
        </div>
        <div className="border-t px-5 py-4 sm:border-t-0">
          <FlowLabel title="Grid" flowStatus={gridDirection === 1 ? "Outputting" : gridDirection === 2 ? "Inputting" : null} />
          <p className="text-5xl font-semibold tabular-nums leading-none">
            {fmtNumber(gridPower != null ? Math.round(gridPower) : null, 0)}
            <span className="ml-1 text-lg font-medium text-muted-foreground">W</span>
          </p>
          <p className="mt-2 text-xs tabular-nums text-muted-foreground">
            {battery ? (
              <>
                pack on Dashboard · {fmtNumber(battery.soc, 0)}% · {fmtNumber(battery.voltage, 1)} V ·{" "}
                {fmtSigned(battery.current, 1)} A
                {batteryDeviceName ? ` · ${batteryDeviceName}` : ""}
              </>
            ) : (
              "pack on Dashboard · no BMS connected"
            )}
          </p>
        </div>
      </div>
    </section>
  );
}

function FlowLabel({ title, flowStatus }: { title: string; flowStatus: string | null }) {
  return (
    <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
      {title}
      {flowStatus ? <span className="ml-auto text-[11px] font-normal normal-case tracking-normal text-amber-600 dark:text-amber-400">{flowStatus}</span> : null}
    </div>
  );
}

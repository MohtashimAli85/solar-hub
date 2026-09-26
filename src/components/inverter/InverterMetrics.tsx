import { cn } from "@/lib/utils";
import { fieldText, fieldValue, fmtNumber } from "@/lib/format";
import type { BatterySnapshot, InverterSnapshot } from "@/lib/types";
import type { ReactNode } from "react";

interface FieldSpec {
  label: string;
  keys: string[];
  unit: string;
  digits?: number;
}

const FIELDS: FieldSpec[] = [
  { label: "Load", keys: ["loadPercentage"], unit: "%", digits: 0 },
  { label: "AC input voltage", keys: ["acInputVoltage"], unit: "V", digits: 1 },
  { label: "AC input frequency", keys: ["acInputFrequency"], unit: "Hz", digits: 1 },
  { label: "Output voltage", keys: ["outputVoltage"], unit: "V", digits: 1 },
  { label: "Output frequency", keys: ["outputFrequency"], unit: "Hz", digits: 1 },
];

const STATUS_FIELDS: { label: string; keys: string[] }[] = [
  { label: "Grid", keys: ["mainsStatus"] },
  { label: "Load", keys: ["loadStatuss", "loadStatus"] },
  { label: "PV", keys: ["pvStatuss", "pvStatus"] },
];

export function InverterMetrics({
  snapshot,
}: {
  snapshot: InverterSnapshot;
  battery?: BatterySnapshot | null;
}) {
  return (
    <div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
        {FIELDS.map((field) => {
          const value = fieldValue(snapshot.fields, field.keys);
          return (
            <InverterMetricCard
              key={field.label}
              label={field.label}
              value={fmtNumber(value, field.digits ?? 1)}
              unit={field.unit}
            />
          );
        })}
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {STATUS_FIELDS.map((status) => {
          const text = fieldText(snapshot.fields, status.keys);
          return (
            <span
              key={status.label}
              className="inline-flex items-center gap-1.5 rounded-full border border-border bg-card px-3 py-1 text-xs text-muted-foreground"
            >
              <span className="font-medium text-foreground">{status.label}</span>
              {text ?? "–"}
            </span>
          );
        })}
      </div>
    </div>
  );
}

function InverterMetricCard({
  label,
  value,
  unit,
  sub,
}: {
  label: string;
  value: string;
  unit: string;
  sub?: ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className={cn("mt-1 text-xl font-semibold tabular-nums")}>
        {value}
        <span className="ml-1 text-sm font-normal text-muted-foreground">{unit}</span>
      </p>
      {sub ? <p className="mt-1 text-xs text-muted-foreground">{sub}</p> : null}
    </div>
  );
}
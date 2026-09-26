import { fieldValue, fmtNumber } from "@/lib/format";
import type { InverterFields } from "@/lib/types";

const STRINGS: { label: string; key: string }[] = [
  { label: "PV1", key: "pv1Power" },
  { label: "PV2", key: "pv2Power" },
  { label: "PV3", key: "pv3Power" },
  { label: "PV4", key: "pv4Power" },
];

export function PvBreakdown({ fields }: { fields: InverterFields }) {
  const strings = STRINGS.map((string) => ({
    ...string,
    value: fieldValue(fields, [string.key]),
  })).filter((string) => string.value != null);

  if (strings.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {strings.map((string) => (
        <span key={string.key} className="tabular-nums">
          {string.label}: {fmtNumber(string.value, 0)} W
        </span>
      ))}
    </div>
  );
}

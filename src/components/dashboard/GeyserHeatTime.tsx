import { useState } from "react";
import { X } from "lucide-react";
import { clock } from "@/components/automation/shared";
import { Button } from "@/components/ui/button";
import { Input, Label } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { fmtDuration } from "@/lib/format";

const WH_PER_LITRE_DEGREE = 1.163;
/** Part of the element's heat that ends up in the water; the rest leaks from the tank while it heats. */
const EFFICIENCY = 0.9;

function heatTime(litres: number, from: number, to: number, watts: number): { minutes: number; kwh: number } | null {
  if (![litres, from, to, watts].every(Number.isFinite) || to <= from || watts <= 0 || litres <= 0) return null;
  const wh = (litres * WH_PER_LITRE_DEGREE * (to - from)) / EFFICIENCY;
  return { minutes: (wh / watts) * 60, kwh: wh / 1000 };
}

/** How long the geyser takes from the water's temperature now to the one wanted, at one power. */
export function GeyserHeatTime({
  litres,
  levels,
  runningW,
  onClose,
}: {
  litres: number;
  levels: number[];
  runningW: number | null;
  onClose: () => void;
}) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("60");
  const nearest =
    runningW != null
      ? levels.reduce((best, level) => (Math.abs(level - runningW) < Math.abs(best - runningW) ? level : best), levels[0])
      : null;
  const [watts, setWatts] = useState(nearest ?? levels[Math.min(1, levels.length - 1)]);
  const result = from.trim() === "" ? null : heatTime(litres, Number(from), Number(to), watts);
  const invalid = from.trim() !== "" && result == null;

  return (
    <div className="space-y-4 rounded-lg border border-border p-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-sm font-medium">Heating time · {litres.toFixed(0)} L tank</p>
        <Button variant="ghost" size="icon" className="-my-2 -mr-2" aria-label="Close heating time" onClick={onClose}>
          <X />
        </Button>
      </div>
      <div className="flex flex-wrap items-end gap-4">
        <div className="w-24 space-y-1.5">
          <Label htmlFor="heat-from">Now (°C)</Label>
          <Input
            id="heat-from"
            type="number"
            inputMode="decimal"
            placeholder="31"
            value={from}
            onChange={(event) => setFrom(event.target.value)}
          />
        </div>
        <div className="w-24 space-y-1.5">
          <Label htmlFor="heat-to">Want (°C)</Label>
          <Input id="heat-to" type="number" inputMode="decimal" value={to} onChange={(event) => setTo(event.target.value)} />
        </div>
        <div className="space-y-1.5">
          <Label>Power</Label>
          <ToggleGroup
            aria-label="Geyser power"
            value={[String(watts)]}
            onValueChange={(next) => {
              if (next[0]) setWatts(Number(next[0]));
            }}
            spacing={0.5}
            className="h-9 items-stretch rounded-md border border-border p-0.5"
          >
            {levels.map((level) => (
              <ToggleGroupItem key={level} value={String(level)} size="sm" className="h-auto rounded px-3 font-medium">
                {level} W
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
      </div>
      <p className="text-sm tabular-nums" aria-live="polite">
        {result ? (
          <>
            <span className="font-semibold">About {fmtDuration(result.minutes)}</span>
            <span className="text-muted-foreground">
              {" "}
              · {result.kwh.toFixed(1)} kWh · hot by {clock(Date.now() + result.minutes * 60_000)} if it starts now
            </span>
          </>
        ) : invalid ? (
          <span className="text-destructive">The wanted temperature must be above the one now.</span>
        ) : (
          <span className="text-muted-foreground">Enter the water's temperature now.</span>
        )}
      </p>
    </div>
  );
}

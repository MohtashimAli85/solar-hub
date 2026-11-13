import { clock } from "@/components/automation/shared";
import { fmtNumber } from "@/lib/format";
import type { SunHour } from "@/lib/types";

export interface LoadLine {
  watts: number;
  label: string;
}

function kw(watts: number): string {
  return `${fmtNumber(watts / 1000, 1)} kW`;
}

/** Spare sun in an hour after the house and the loads already on. */
function freeWatts(hour: SunHour): number {
  return Math.max(0, hour.sun_w - hour.house_w - hour.loads_w);
}

/**
 * "Spare now 2.2 kW · geyser 2000 W fits until about 1 PM": the biggest
 * still-needed load that fits now, and the hour the spare sun drops below it.
 */
export function spareNowText(hours: SunHour[], lines: LoadLine[]): string | null {
  const now = hours[0];
  if (!now) return null;
  const free = freeWatts(now);
  const spare = `Spare now ${kw(free)}`;
  if (lines.length === 0) return spare;
  const best = lines.filter((line) => line.watts <= free).sort((a, b) => b.watts - a.watts)[0];
  if (!best) return `${spare} · nothing needed fits`;
  const until = hours.find((hour) => freeWatts(hour) < best.watts);
  const fits = `${best.label} ${fmtNumber(best.watts, 0)} W fits`;
  return `${spare} · ${until ? `${fits} until about ${clock(until.at)}` : `${fits} until sunset`}`;
}

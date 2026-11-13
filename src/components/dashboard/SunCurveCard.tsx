import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { RefreshCw, SunMedium } from "lucide-react";
import { Button } from "@/components/ui/button";
import { CHART_FONT, ChartTooltipBox, chartStep, clock, toMs, VIZ } from "@/components/automation/shared";
import { fmtNumber } from "@/lib/format";
import type { MeasuredPv, PvPoint } from "@/lib/types";

const USEFUL_W = 100;
const HOUR_MS = 3_600_000;
const FIVE_MIN_MS = 300_000;
const REAL = "var(--foreground)";

interface Row {
  at: number;
  before?: number;
  ahead?: number;
  real?: number;
  heldBack?: boolean;
  now?: boolean;
}

interface Watts {
  at: number;
  watts: number;
}

function kwText(watts: number): string {
  return watts >= 1000 ? `${fmtNumber(watts / 1000, 1)} kW` : `${Math.round(watts)} W`;
}

function roundTime(ms: number): number {
  return Math.round(ms / FIVE_MIN_MS) * FIVE_MIN_MS;
}

function crossings(points: Watts[], watts: number): { from: number | null; to: number | null } {
  let from: number | null = null;
  let to: number | null = null;
  for (let index = 1; index < points.length; index++) {
    const a = points[index - 1];
    const b = points[index];
    const cross = a.at + ((watts - a.watts) / (b.watts - a.watts)) * (b.at - a.at);
    if (a.watts < watts && b.watts >= watts && from == null) from = cross;
    if (a.watts >= watts && b.watts < watts) to = cross;
  }
  return { from, to };
}

function wattsAt(points: Watts[], at: number): number | null {
  for (let index = 1; index < points.length; index++) {
    const a = points[index - 1];
    const b = points[index];
    if (at >= a.at && at <= b.at) return a.watts + ((at - a.at) / (b.at - a.at)) * (b.watts - a.watts);
  }
  return null;
}

function parse<T extends { at: string; watts: number }>(points: T[] | undefined): (T & { ms: number })[] {
  return (points ?? []).flatMap((point) => {
    const ms = toMs(point.at);
    return ms == null ? [] : [{ ...point, ms }];
  });
}

function LegendLine({ color, dashed, label }: { color: string; dashed?: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <svg width="16" height="6" aria-hidden>
        <line x1="0" y1="3" x2="16" y2="3" stroke={color} strokeWidth="2" strokeDasharray={dashed ? "3 3" : undefined} />
      </svg>
      {label}
    </span>
  );
}

/** Today's expected panel output against what the panels really gave, so the family can see when the sun is good for loads. */
export function SunCurveCard({
  forecast,
  measured,
  fetchedAt,
  refreshing,
  onRefresh,
  now = Date.now(),
}: {
  forecast: PvPoint[] | undefined;
  measured: MeasuredPv[] | undefined;
  fetchedAt: string | null | undefined;
  refreshing: boolean;
  onRefresh: () => void;
  now?: number;
}) {
  const all: Watts[] = parse(forecast).map((point) => ({ at: point.ms, watts: Math.max(0, point.watts) }));
  const lit = all.map((point, index) => (point.watts > 0 ? index : -1)).filter((index) => index >= 0);
  if (lit.length === 0) return null;
  const sunny = all.slice(Math.max(0, lit[0] - 1), Math.min(all.length, lit[lit.length - 1] + 2));
  const start = sunny[0].at;
  const end = sunny[sunny.length - 1].at;
  const real = parse(measured).filter((point) => point.ms >= start && point.ms <= end);

  const peak = sunny.reduce((best, point) => (point.watts > best.watts ? point : best), sunny[0]);
  const { from, to } = crossings(sunny, USEFUL_W);
  const nowWatts = wattsAt(sunny, now);
  const lastReal = real.length > 0 && now - real[real.length - 1].ms < 30 * 60_000 ? real[real.length - 1] : null;

  const rows: Row[] = [
    ...sunny.map((point) => ({
      at: point.at,
      before: point.at <= now ? point.watts : undefined,
      ahead: point.at >= now ? point.watts : undefined,
    })),
    ...real.map((point) => ({ at: point.ms, real: point.watts, heldBack: point.held_back })),
  ];
  if (nowWatts != null) rows.push({ at: now, before: nowWatts, ahead: nowWatts, now: true });
  rows.sort((a, b) => a.at - b.at);

  const highest = Math.max(peak.watts, ...real.map((point) => point.watts)) / 1000;
  const step = chartStep(highest);
  const top = Math.max(step, Math.ceil(highest / step) * step);
  const yTicks = Array.from({ length: Math.round(top / step) + 1 }, (_, index) => index * step * 1000);
  const xTicks: number[] = [];
  for (let tick = Math.ceil(start / HOUR_MS) * HOUR_MS; tick <= end; tick += HOUR_MS) {
    if (new Date(tick).getHours() % 3 === 0) xTicks.push(tick);
  }

  const window =
    from != null && to != null
      ? `${USEFUL_W} W or more from ${clock(roundTime(from))} to ${clock(roundTime(to))}`
      : `The panels won't reach ${USEFUL_W} W today`;
  const summary = `${window} · peak about ${kwText(peak.watts)} around ${clock(peak.at)}`;
  const showNow = nowWatts != null || (now >= start && now <= end);

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:p-5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3 className="inline-flex items-center gap-2 text-label font-semibold uppercase tracking-wide text-muted-foreground">
          <SunMedium className="size-4" aria-hidden /> Sun today
        </h3>
        <span className="inline-flex items-center gap-2 text-xs tabular-nums text-muted-foreground">
          {fetchedAt ? `Forecast from ${clock(fetchedAt)}` : null}
          <Button variant="ghost" size="sm" onClick={onRefresh} disabled={refreshing}>
            <RefreshCw className={refreshing ? "animate-spin motion-reduce:animate-none" : undefined} />
            {refreshing ? "Updating…" : "Update"}
          </Button>
        </span>
      </div>
      <div className="space-y-0.5">
        <p className="text-sm tabular-nums">{summary}</p>
        {nowWatts != null ? (
          <p className="text-xs tabular-nums text-muted-foreground">
            Now: forecast {kwText(nowWatts)}
            {lastReal ? ` · real ${kwText(lastReal.watts)}` : ""}
          </p>
        ) : null}
      </div>
      <div className="h-44" role="img" aria-label={`Expected solar output today against the real output so far. ${summary}.`}>
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={rows} margin={{ top: 8, right: 0, bottom: 0, left: 0 }}>
            <CartesianGrid vertical={false} stroke={VIZ.grid} />
            <XAxis
              dataKey="at"
              type="number"
              scale="time"
              domain={[start, end]}
              ticks={xTicks}
              tickFormatter={(value: number) => clock(value).replace(":00", "")}
              tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
              tickLine={false}
              axisLine={false}
            />
            <YAxis
              orientation="right"
              domain={[0, top * 1000]}
              ticks={yTicks}
              tickFormatter={(value: number) => `${fmtNumber(value / 1000, step < 1 ? 1 : 0)} kW`}
              tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
              tickLine={false}
              axisLine={false}
              width={44}
            />
            <ReferenceLine
              y={USEFUL_W}
              stroke={VIZ.contextStrong}
              strokeDasharray="3 3"
              label={{ value: `${USEFUL_W} W`, position: "insideBottomLeft", fontSize: CHART_FONT, fill: VIZ.ink }}
            />
            {showNow ? (
              <ReferenceLine
                x={now}
                stroke={VIZ.contextStrong}
                label={{ value: "now", position: "insideTopLeft", fontSize: CHART_FONT, fill: VIZ.ink }}
              />
            ) : null}
            <Tooltip
              cursor={{ stroke: VIZ.contextStrong, strokeWidth: 1 }}
              content={({ active, label }) => {
                if (!active || typeof label !== "number") return null;
                const expected = wattsAt(sunny, label);
                const nearest = real.reduce<(typeof real)[number] | null>(
                  (best, point) =>
                    Math.abs(point.ms - label) < 10 * 60_000 && (!best || Math.abs(point.ms - label) < Math.abs(best.ms - label))
                      ? point
                      : best,
                  null,
                );
                return (
                  <ChartTooltipBox>
                    <p className="font-medium">{clock(label)}</p>
                    {expected != null ? (
                      <p className="text-muted-foreground">
                        Forecast <span className="text-foreground">{kwText(expected)}</span>
                      </p>
                    ) : null}
                    {nearest ? (
                      <p className="text-muted-foreground">
                        Real <span className="text-foreground">{kwText(nearest.watts)}</span>
                        {nearest.held_back ? " · battery full, panels held back" : ""}
                      </p>
                    ) : null}
                  </ChartTooltipBox>
                );
              }}
            />
            <Line
              dataKey="before"
              type="monotone"
              stroke={VIZ.accent}
              strokeWidth={2}
              strokeDasharray="4 4"
              dot={false}
              activeDot={false}
              connectNulls
              isAnimationActive={false}
            />
            <Line
              dataKey="ahead"
              type="monotone"
              stroke={VIZ.accent}
              strokeWidth={2}
              dot={false}
              activeDot={false}
              connectNulls
              isAnimationActive={false}
            />
            <Line
              dataKey="real"
              type="linear"
              stroke={REAL}
              strokeWidth={2}
              dot={false}
              activeDot={false}
              connectNulls
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-micro text-muted-foreground">
        <LegendLine color={VIZ.accent} label="Forecast" />
        <LegendLine color={REAL} label="Real so far" />
        <span>Real drops when the battery is full: the panels are held back.</span>
      </div>
    </section>
  );
}

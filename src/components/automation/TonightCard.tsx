import { CartesianGrid, ComposedChart, Line, ReferenceArea, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { AutomationInsights } from "@/lib/types";
import { fmtNumber } from "@/lib/format";
import { CHART_FONT, VIZ, ChartTooltipBox, Chip, EmptyNote, SectionTitle, clock, smartLoadText, toMs } from "./shared";

interface Row {
  t: number;
  measured?: number;
  projected?: number;
}

function mergeSeries(insights: AutomationInsights): Row[] {
  const rows = new Map<number, Row>();
  const measured = insights.trajectory_is_preview ? [] : insights.actual_soc;
  for (const point of measured) {
    const t = toMs(point.at);
    if (t != null) rows.set(t, { ...rows.get(t), t, measured: point.soc });
  }
  for (const point of insights.trajectory) {
    const t = toMs(point.at);
    if (t != null) rows.set(t, { ...rows.get(t), t, projected: Math.round(point.soc * 10) / 10 });
  }
  return [...rows.values()].sort((a, b) => a.t - b.t);
}

function SeriesKey({ measured }: { measured: boolean }) {
  return (
    <div className="flex items-center gap-4 text-micro text-muted-foreground">
      <span className={measured ? "inline-flex items-center gap-1.5" : "hidden"}>
        <svg width="18" height="6" aria-hidden>
          <line x1="0" y1="3" x2="18" y2="3" stroke={VIZ.accent} strokeWidth="2" />
        </svg>
        Measured
      </span>
      <span className="inline-flex items-center gap-1.5">
        <svg width="18" height="6" aria-hidden>
          <line x1="0" y1="3" x2="18" y2="3" stroke={VIZ.accent} strokeWidth="2" strokeDasharray="4 3" opacity="0.75" />
        </svg>
        Projected
      </span>
    </div>
  );
}

export function TonightCard({ insights }: { insights: AutomationInsights }) {
  const rows = mergeSeries(insights);
  const preview = insights.trajectory_is_preview;
  const sunrise = toMs(insights.next_sunrise);
  const reserve = insights.reserve_soc;
  const floor = insights.floor_soc;
  const lastProjected = insights.trajectory[insights.trajectory.length - 1]?.soc ?? null;
  const basisNote =
    insights.trajectory_basis === "history"
      ? `Projection follows your routine from the last ${insights.routine.nights_with_data} night${insights.routine.nights_with_data === 1 ? "" : "s"}.`
      : "No routine recorded yet — the projection assumes the current load all night.";

  return (
    <Card>
      <CardHeader className="gap-3">
        <SectionTitle
          eyebrow={preview ? `Preview · planning starts at sunset, ${clock(insights.sunset)}` : "Tonight"}
          title={preview ? "How tonight could go on battery" : "Battery through the night"}
          aside={<SeriesKey measured={!preview} />}
        />
        <div className="flex flex-wrap gap-2">
          {reserve != null ? (
            <Chip tone="accent">On battery until {Math.round(reserve)}%</Chip>
          ) : null}
          {insights.reserve_eta ? <Chip>Reaches reserve ≈ {clock(insights.reserve_eta)}</Chip> : null}
          {insights.backup_hours != null ? <Chip>Backup kept ≈ {fmtNumber(insights.backup_hours, 1)} h</Chip> : null}
          {!preview && smartLoadText(insights.smart_load, "night") ? <Chip>{smartLoadText(insights.smart_load, "night")}</Chip> : null}
          {reserve == null && lastProjected != null ? (
            <Chip>All night on battery would end ≈ {Math.round(lastProjected)}%</Chip>
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="space-y-2">
        {rows.length < 2 ? (
          <EmptyNote>Waiting for battery and sunrise data to project the night.</EmptyNote>
        ) : (
          <div className="h-52 sm:h-64" role="img" aria-label={`Battery state of charge through the night. ${basisNote}`}>
            <ResponsiveContainer width="100%" height="100%">
              <ComposedChart data={rows} margin={{ top: 8, right: 8, bottom: 0, left: -8 }}>
                <CartesianGrid vertical={false} stroke={VIZ.grid} />
                <ReferenceArea y1={0} y2={floor} fill={VIZ.context} fillOpacity={0.5} ifOverflow="hidden" />
                <XAxis
                  dataKey="t"
                  type="number"
                  scale="time"
                  domain={["dataMin", "dataMax"]}
                  tickFormatter={(t: number) => clock(t)}
                  tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
                  tickLine={false}
                  axisLine={false}
                  minTickGap={48}
                />
                <YAxis
                  domain={[0, 100]}
                  ticks={[0, 25, 50, 75, 100]}
                  tickFormatter={(v: number) => `${v}%`}
                  tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
                  tickLine={false}
                  axisLine={false}
                  width={44}
                />
                {reserve != null ? (
                  <ReferenceLine
                    y={reserve}
                    stroke={VIZ.contextStrong}
                    strokeDasharray="2 4"
                    strokeWidth={1.5}
                    label={{ value: `Reserve ${Math.round(reserve)}%`, position: "insideTopRight", fontSize: CHART_FONT, fill: VIZ.ink }}
                  />
                ) : null}
                <ReferenceLine
                  y={floor}
                  stroke="transparent"
                  label={{ value: `Floor ${Math.round(floor)}%`, position: "insideBottomLeft", fontSize: CHART_FONT, fill: VIZ.ink }}
                />
                {sunrise != null ? (
                  <ReferenceLine
                    x={sunrise}
                    stroke={VIZ.contextStrong}
                    label={{ value: "Sunrise", position: "insideTopRight", fontSize: CHART_FONT, fill: VIZ.ink }}
                  />
                ) : null}
                <Tooltip
                  cursor={{ stroke: VIZ.contextStrong, strokeWidth: 1 }}
                  content={({ active, payload, label }) =>
                    active && payload?.length ? (
                      <ChartTooltipBox>
                        <p className="font-medium">{clock(label as number)}</p>
                        {payload.map((entry) => (
                          <p key={String(entry.dataKey)} className="text-muted-foreground">
                            {entry.dataKey === "measured" ? "Measured" : "Projected"}{" "}
                            <span className="text-foreground">{fmtNumber(entry.value as number, 0)}%</span>
                          </p>
                        ))}
                      </ChartTooltipBox>
                    ) : null
                  }
                />
                <Line
                  dataKey="measured"
                  stroke={VIZ.accent}
                  strokeWidth={2}
                  dot={false}
                  connectNulls
                  isAnimationActive={false}
                />
                <Line
                  dataKey="projected"
                  stroke={VIZ.accent}
                  strokeWidth={2}
                  strokeDasharray="5 4"
                  strokeOpacity={0.75}
                  dot={false}
                  activeDot={{ r: 4, stroke: "var(--card)", strokeWidth: 2 }}
                  connectNulls
                  isAnimationActive={false}
                />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
        )}
        <p className="text-micro text-muted-foreground">{basisNote}</p>
      </CardContent>
    </Card>
  );
}

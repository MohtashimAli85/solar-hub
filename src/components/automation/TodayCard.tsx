import { Bar, BarChart, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { fmtNumber } from "@/lib/format";
import type { AutomationInsights, ProjectionMethod } from "@/lib/types";
import { CHART_FONT, VIZ, ChartTooltipBox, EmptyNote, SectionTitle, Stat, clock, smartLoadText, toMs } from "./shared";

const METHOD_NOTE: Record<ProjectionMethod, string> = {
  pv_array: "Projected from your array size and the forecast sun, minus your usual house load, up to the inverter's charge limit.",
  pv_headroom:
    "Projected from what the panels make now, scaled by the forecast sun, minus your usual house load — so a big load that's only temporary (like an EV charging) doesn't count against it. Capped at the inverter's charge limit.",
  charge_scaling: "Projected from the current charge current, scaled by the forecast sun for each hour.",
  constant_charge: "No forecast right now — projected by holding the current charge current until sunset.",
  after_sunset: "The sun has set — nothing will charge the battery until morning.",
};

export function TodayCard({ insights }: { insights: AutomationInsights }) {
  const day = insights.day;
  if (!day) {
    return (
      <Card>
        <CardHeader>
          <SectionTitle eyebrow="Today" title="Will the battery fill before sunset?" />
        </CardHeader>
        <CardContent>
          <EmptyNote>Waiting for a battery reading to project the day.</EmptyNote>
        </CardContent>
      </Card>
    );
  }

  const afterSunset = day.method === "after_sunset";
  const fills = day.soc_at_sunset >= 99.5;
  const verdict = afterSunset
    ? "Sun has set · the night plan takes over"
    : fills
      ? `On pace to be full by ~${clock(day.full_at)}`
      : `On pace for ≈ ${Math.round(day.soc_at_sunset)}% by sunset`;
  const now = toMs(insights.updated_at) ?? Date.now();
  const hours = (insights.weather?.hours_until_sunset ?? []).map((hour) => ({
    t: (toMs(hour.at) ?? 0) - 3_600_000,
    radiation: hour.radiation_w_m2 ?? 0,
    cloud: hour.cloud_pct,
  }));

  return (
    <Card>
      <CardHeader className="gap-3">
        <SectionTitle eyebrow="Today" title="Will the battery fill before sunset?" />
        <p className="text-sm font-medium">{verdict}</p>
        {smartLoadText(insights.smart_load, "day") ? (
          <p className="text-xs text-muted-foreground">{smartLoadText(insights.smart_load, "day")}</p>
        ) : null}
      </CardHeader>
      <CardContent className="space-y-4">
        <div
          className="relative h-3 overflow-hidden rounded-full bg-muted"
          role="img"
          aria-label={`Battery ${Math.round(day.soc_now)}% now, projected ${Math.round(day.soc_at_sunset)}% at sunset`}
        >
          <div className="absolute inset-y-0 left-0 bg-[var(--viz-context-strong)]" style={{ width: `${day.soc_at_sunset}%` }} />
          <div className="absolute inset-y-0 left-0 border-r-2 border-card bg-[var(--viz-accent)]" style={{ width: `${day.soc_now}%` }} />
        </div>
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Stat label="Now" value={`${Math.round(day.soc_now)}%`} hint={`${fmtNumber(day.ah_to_full, 0)} Ah to full`} />
          <Stat label="At sunset" value={`${Math.round(day.soc_at_sunset)}%`} hint={clock(insights.sunset)} />
          <Stat
            label="Charging"
            value={`${fmtNumber(day.charge_a, 1)} A`}
            hint={day.max_charge_a != null ? `can take up to ${fmtNumber(day.max_charge_a, 0)} A` : undefined}
          />
          <Stat label="Sun left" value={`${fmtNumber(day.hours_of_sun_left, 1)} h`} />
        </div>
        {hours.length > 0 && !afterSunset ? (
          <div className="h-28" role="img" aria-label="Forecast solar radiation for each remaining hour of daylight">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={hours} margin={{ top: 4, right: 0, bottom: 0, left: -8 }} barCategoryGap={2}>
                <XAxis
                  dataKey="t"
                  tickFormatter={(t: number) => clock(t)}
                  tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
                  tickLine={false}
                  axisLine={false}
                  interval="preserveStartEnd"
                  minTickGap={32}
                />
                <YAxis hide domain={[0, "auto"]} />
                <Tooltip
                  cursor={{ fill: VIZ.grid }}
                  content={({ active, payload }) => {
                    const row = payload?.[0]?.payload as (typeof hours)[number] | undefined;
                    return active && row ? (
                      <ChartTooltipBox>
                        <p className="font-medium">{clock(row.t)}</p>
                        <p className="text-muted-foreground">
                          Sun <span className="text-foreground">{fmtNumber(row.radiation, 0)} W/m²</span>
                        </p>
                        <p className="text-muted-foreground">
                          Cloud <span className="text-foreground">{fmtNumber(row.cloud, 0)}%</span>
                        </p>
                      </ChartTooltipBox>
                    ) : null;
                  }}
                />
                <Bar dataKey="radiation" radius={[4, 4, 0, 0]} isAnimationActive={false}>
                  {hours.map((row) => (
                    <Cell key={row.t} fill={row.t <= now && now < row.t + 3_600_000 ? VIZ.accent : VIZ.contextStrong} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </div>
        ) : null}
        <p className="text-micro text-muted-foreground">{METHOD_NOTE[day.method]}</p>
      </CardContent>
    </Card>
  );
}

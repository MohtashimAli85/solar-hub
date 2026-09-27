import { openUrl } from "@tauri-apps/plugin-opener";
import { Bar, BarChart, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { fmtNumber } from "@/lib/format";
import type { AutomationInsights } from "@/lib/types";
import { ChartTooltipBox, dayLabel, EmptyNote, SectionTitle, Stat, VIZ } from "./shared";

function dayName(date: string, reference: string | null): string {
  const target = new Date(`${date}T12:00:00`);
  const base = reference ? new Date(reference) : new Date();
  const tomorrow = new Date(base.getFullYear(), base.getMonth(), base.getDate() + 1, 12);
  const today = new Date(base.getFullYear(), base.getMonth(), base.getDate(), 12);
  if (target.toDateString() === tomorrow.toDateString()) return "Tomorrow";
  if (target.toDateString() === today.toDateString()) return "Today";
  return target.toLocaleDateString("en-US", { weekday: "long" });
}

function compare(value: number | null | undefined, average: number | null | undefined): string | undefined {
  if (value == null || average == null || average === 0) return undefined;
  const delta = Math.round(((value - average) / average) * 100);
  if (Math.abs(delta) < 5) return "about the same as this week";
  return `${Math.abs(delta)}% ${delta > 0 ? "more" : "less"} than this week`;
}

function Credit() {
  return (
    <p className="text-[11px] text-muted-foreground">
      Weather data by{" "}
      <button
        type="button"
        className="underline underline-offset-2 hover:text-foreground"
        onClick={() => void openUrl("https://open-meteo.com/")}
      >
        Open-Meteo.com
      </button>{" "}
      (CC BY 4.0)
    </p>
  );
}

export function WeatherCard({ insights }: { insights: AutomationInsights }) {
  const weather = insights.weather;
  if (!weather) {
    return (
      <Card>
        <CardHeader>
          <SectionTitle eyebrow="Sun & season" title="Forecast" />
        </CardHeader>
        <CardContent className="space-y-3">
          <EmptyNote>Forecast unavailable right now. The agent keeps a larger reserve until it's back.</EmptyNote>
          <Credit />
        </CardContent>
      </Card>
    );
  }

  const next = weather.next_day;
  const bars = [
    ...weather.recent_days.map((day) => ({ key: day.date, label: dayLabel(day.date), radiation: day.radiation_kwh_m2, fullAt: day.full_at, maxSoc: day.max_soc, forecast: false })),
    ...(next ? [{ key: next.date, label: dayLabel(next.date), radiation: next.radiation_kwh_m2, fullAt: null, maxSoc: null, forecast: true }] : []),
  ];
  const tempDelta =
    weather.tonight_min_temp_c != null && weather.recent_nights_min_temp_c != null
      ? weather.tonight_min_temp_c - weather.recent_nights_min_temp_c
      : null;

  return (
    <Card>
      <CardHeader className="gap-2">
        <SectionTitle eyebrow="Sun & season" title={next ? `${dayName(next.date, insights.updated_at)}'s sun decides how deep tonight can go` : "Forecast"} />
      </CardHeader>
      <CardContent className="space-y-4">
        {next ? (
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <Stat
              label="Sunshine"
              value={`${fmtNumber(next.sunshine_h, 1)} h`}
            />
            <Stat
              label="Solar energy"
              value={`${fmtNumber(next.radiation_kwh_m2, 1)} kWh/m²`}
              hint={compare(next.radiation_kwh_m2, weather.recent_avg_radiation_kwh_m2)}
            />
            <Stat label="Cloud" value={`${fmtNumber(next.cloud_pct, 0)}%`} hint="daytime average" />
            <Stat label="Rain chance" value={`${fmtNumber(next.rain_prob_pct, 0)}%`} />
          </div>
        ) : null}

        {bars.length > 0 ? (
          <div className="h-32" role="img" aria-label="Daily solar energy for the last week and the forecast day">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={bars} margin={{ top: 4, right: 0, bottom: 0, left: -8 }} barCategoryGap={6}>
                <XAxis dataKey="label" tick={{ fontSize: 10, fill: VIZ.ink }} tickLine={false} axisLine={false} />
                <YAxis hide domain={[0, "auto"]} />
                <Tooltip
                  cursor={{ fill: VIZ.grid }}
                  content={({ active, payload }) => {
                    const row = payload?.[0]?.payload as (typeof bars)[number] | undefined;
                    return active && row ? (
                      <ChartTooltipBox>
                        <p className="font-medium">
                          {row.label}
                          {row.forecast ? " · forecast" : ""}
                        </p>
                        <p className="text-muted-foreground">
                          Solar energy <span className="text-foreground">{fmtNumber(row.radiation, 1)} kWh/m²</span>
                        </p>
                        {!row.forecast ? (
                          <p className="text-muted-foreground">
                            Battery{" "}
                            <span className="text-foreground">
                              {row.fullAt ? `full by ${row.fullAt}` : row.maxSoc != null ? `peaked at ${Math.round(row.maxSoc)}%` : "not recorded"}
                            </span>
                          </p>
                        ) : null}
                      </ChartTooltipBox>
                    ) : null;
                  }}
                />
                <Bar dataKey="radiation" radius={[4, 4, 0, 0]} isAnimationActive={false}>
                  {bars.map((row) => (
                    <Cell key={row.key} fill={row.forecast ? VIZ.accent : VIZ.contextStrong} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </div>
        ) : null}

        <p className="text-sm text-muted-foreground">
          Tonight's low <span className="font-medium text-foreground">{fmtNumber(weather.tonight_min_temp_c, 1)}°C</span>
          {tempDelta != null
            ? Math.abs(tempDelta) < 0.5
              ? " — same as recent nights."
              : ` — ${fmtNumber(Math.abs(tempDelta), 1)}° ${tempDelta < 0 ? "cooler, so less fan and AC load" : "warmer, so more fan and AC load"} than recent nights.`
            : "."}
          {weather.expected_pv_kwh_next_day != null
            ? ` Your array should make about ${fmtNumber(weather.expected_pv_kwh_next_day, 1)} kWh tomorrow.`
            : ""}
        </p>
        <Credit />
      </CardContent>
    </Card>
  );
}

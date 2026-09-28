import { Bar, ComposedChart, Line, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { fmtNumber } from "@/lib/format";
import type { AutomationInsights } from "@/lib/types";
import { CHART_FONT, VIZ, ChartTooltipBox, EmptyNote, SectionTitle, hourLabel } from "./shared";

const NIGHT_HOURS = [18, 19, 20, 21, 22, 23, 0, 1, 2, 3, 4, 5, 6, 7];
const NIGHTS_NEEDED = 7;

export function RoutineCard({ insights }: { insights: AutomationInsights }) {
  const { routine } = insights;
  const rows = NIGHT_HOURS.map((hour) => ({
    hour,
    typical: routine.typical.find((h) => h.hour === hour)?.load_w ?? null,
    tonight: routine.tonight.find((h) => h.hour === hour)?.load_w ?? null,
  }));
  const hasTypical = routine.nights_with_data > 0;
  const hasTonight = routine.tonight.length > 0;

  return (
    <Card>
      <CardHeader className="gap-2">
        <SectionTitle
          eyebrow="Household routine"
          title={routine.quiet_by != null ? `Usually quiet by ~${hourLabel(routine.quiet_by)}` : "Evening and night load"}
          aside={
            hasTypical ? (
              <div className="flex items-center gap-4 text-micro text-muted-foreground">
                <span className="inline-flex items-center gap-1.5">
                  <span className="inline-block h-2.5 w-3 rounded-[2px] bg-[var(--viz-context-strong)]" aria-hidden />
                  Typical (last {routine.nights_with_data} nights)
                </span>
                {hasTonight ? (
                  <span className="inline-flex items-center gap-1.5">
                    <svg width="18" height="6" aria-hidden>
                      <line x1="0" y1="3" x2="18" y2="3" stroke={VIZ.accent} strokeWidth="2" />
                    </svg>
                    Tonight
                  </span>
                ) : null}
              </div>
            ) : null
          }
        />
      </CardHeader>
      <CardContent className="space-y-2">
        {!hasTypical && !hasTonight ? (
          <EmptyNote>
            Learning your routine — 0 of {NIGHTS_NEEDED} nights recorded. Keep the app running overnight; each night teaches the agent
            when the house goes quiet.
          </EmptyNote>
        ) : (
          <>
            <div className="h-40 sm:h-44" role="img" aria-label="Typical house load per hour of the night compared with tonight">
              <ResponsiveContainer width="100%" height="100%">
                <ComposedChart data={rows} margin={{ top: 4, right: 0, bottom: 0, left: -8 }} barCategoryGap={2}>
                  <XAxis
                    dataKey="hour"
                    tickFormatter={(hour: number) => hourLabel(hour)}
                    tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
                    tickLine={false}
                    axisLine={false}
                    interval={1}
                  />
                  <YAxis
                    tickFormatter={(v: number) => `${fmtNumber(v, 0)} W`}
                    tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
                    tickLine={false}
                    axisLine={false}
                    width={52}
                  />
                  <Tooltip
                    cursor={{ fill: VIZ.grid }}
                    content={({ active, payload }) => {
                      const row = payload?.[0]?.payload as (typeof rows)[number] | undefined;
                      return active && row ? (
                        <ChartTooltipBox>
                          <p className="font-medium">{hourLabel(row.hour)}</p>
                          <p className="text-muted-foreground">
                            Typical <span className="text-foreground">{fmtNumber(row.typical, 0)} W</span>
                          </p>
                          {row.tonight != null ? (
                            <p className="text-muted-foreground">
                              Tonight <span className="text-foreground">{fmtNumber(row.tonight, 0)} W</span>
                            </p>
                          ) : null}
                        </ChartTooltipBox>
                      ) : null;
                    }}
                  />
                  <Bar dataKey="typical" fill={VIZ.contextStrong} radius={[4, 4, 0, 0]} isAnimationActive={false} />
                  <Line
                    dataKey="tonight"
                    stroke={VIZ.accent}
                    strokeWidth={2}
                    dot={{ r: 3, fill: VIZ.accent, stroke: "var(--card)", strokeWidth: 2 }}
                    connectNulls={false}
                    isAnimationActive={false}
                  />
                </ComposedChart>
              </ResponsiveContainer>
            </div>
            {routine.nights_with_data < NIGHTS_NEEDED ? (
              <p className="text-micro text-muted-foreground">
                {routine.nights_with_data} of {NIGHTS_NEEDED} nights recorded — the routine sharpens as more nights come in.
              </p>
            ) : (
              <p className="text-micro text-muted-foreground">
                Median of the last {NIGHTS_NEEDED} nights, so it follows the season as nights cool.
              </p>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}

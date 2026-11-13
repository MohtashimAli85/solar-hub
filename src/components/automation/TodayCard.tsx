import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { fmtNumber } from "@/lib/format";
import type { AutomationInsights, ProjectionMethod } from "@/lib/types";
import { EmptyNote, SectionTitle, Stat, clock, smartLoadText } from "./shared";

const METHOD_NOTE: Record<ProjectionMethod, string> = {
  pv_array:
    "Projected from the expected sun, corrected by what the panels make now, minus what the house uses now for the next hour and its usual load after that, up to the inverter's charge limit.",
  pv_headroom:
    "Projected from what the panels make now, scaled by the forecast sun, minus what the house uses now for the next hour and its usual load after that. Capped at the inverter's charge limit.",
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
        <p className="text-micro text-muted-foreground">{METHOD_NOTE[day.method]}</p>
      </CardContent>
    </Card>
  );
}

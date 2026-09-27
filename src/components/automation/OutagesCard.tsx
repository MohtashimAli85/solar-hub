import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { AutomationInsights } from "@/lib/types";
import { PlugZap } from "lucide-react";
import { clock, EmptyNote, minutesText, SectionTitle } from "./shared";

function when(local: string): string {
  const date = new Date(local);
  return `${date.toLocaleDateString("en-US", { weekday: "short", day: "numeric", month: "short" })}, ${clock(local)}`;
}

export function OutagesCard({ insights }: { insights: AutomationInsights }) {
  const outages = [...insights.outages].reverse();
  const total = outages.reduce((sum, outage) => sum + outage.minutes, 0);

  return (
    <Card>
      <CardHeader className="gap-1">
        <SectionTitle
          eyebrow="Load-shedding"
          title={outages.length ? `${outages.length} outage${outages.length === 1 ? "" : "s"} · ${minutesText(total)} this week` : "No outages this week"}
        />
        <p className="text-xs text-muted-foreground">More outages → the agent keeps a bigger backup.</p>
      </CardHeader>
      <CardContent>
        {outages.length === 0 ? (
          <EmptyNote>No grid outages recorded in the last 7 days.</EmptyNote>
        ) : (
          <ul className="divide-y divide-border">
            {outages.slice(0, 8).map((outage) => (
              <li key={outage.start} className="flex items-center justify-between gap-3 py-2 text-sm">
                <span className="inline-flex items-center gap-2">
                  <PlugZap className="size-3.5 text-muted-foreground" aria-hidden />
                  {when(outage.start)}
                </span>
                <span className="tabular-nums text-muted-foreground">
                  {outage.ongoing ? (
                    <span className="font-medium text-amber-600 dark:text-amber-400">ongoing · </span>
                  ) : null}
                  {minutesText(outage.minutes)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

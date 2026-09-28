import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { AutomationDecision } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Moon, Sun } from "lucide-react";
import { clock, EmptyNote, modelLabel, SectionTitle, Skeleton } from "./shared";

const MODE_LABELS: Record<string, string> = {
  sbg: "Battery",
  solar: "Solar",
  boost_sbg: "Battery · oven boost",
  smart_load_on: "Smart load on · heavy loads cut",
  smart_load_off: "Smart load off · everything runs",
};

function outcome(decision: AutomationDecision): string {
  if (!decision.applied) return "Kept";
  return decision.dry_run ? "Would switch" : "Switched";
}

function dateLabel(local: string): string {
  const date = new Date(local);
  const today = new Date();
  const yesterday = new Date(today.getTime() - 86_400_000);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString("en-US", { weekday: "short", day: "numeric", month: "short" });
}

interface DecisionTimelineProps {
  decisions: AutomationDecision[] | undefined;
  canShowMore: boolean;
  onShowMore: () => void;
}

export function DecisionTimeline({ decisions, canShowMore, onShowMore }: DecisionTimelineProps) {
  return (
    <Card>
      <CardHeader>
        <SectionTitle eyebrow="Decisions" title="What the agent decided, and why" />
      </CardHeader>
      <CardContent className="space-y-3">
        {decisions == null ? (
          <div className="space-y-2">
            <Skeleton className="h-10" />
            <Skeleton className="h-10" />
          </div>
        ) : decisions.length === 0 ? (
          <EmptyNote>No decisions yet. The first one comes at the next check.</EmptyNote>
        ) : (
          <ol className="space-y-0">
            {decisions.map((decision) => {
              const Icon = decision.window === "night" ? Moon : Sun;
              const label = MODE_LABELS[decision.mode] ?? decision.mode;
              return (
                <li
                  key={`${decision.at}-${decision.mode}-${decision.reason.slice(0, 12)}`}
                  className="grid grid-cols-[3.75rem_1fr] gap-2 border-l sm:grid-cols-[4.5rem_1fr] sm:gap-3 border-border py-2.5 pl-4 relative"
                >
                  <span
                    className={cn(
                      "absolute -left-[5px] top-4 size-2.5 rounded-full border-2 border-card",
                      decision.applied ? "bg-[var(--viz-accent)]" : "bg-[var(--viz-context-strong)]",
                    )}
                    aria-hidden
                  />
                  <div className="text-xs tabular-nums text-muted-foreground">
                    <p className="font-medium text-foreground">{clock(decision.at)}</p>
                    <p>{dateLabel(decision.at)}</p>
                  </div>
                  <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-1.5 text-xs">
                      <Icon className="size-3.5 text-muted-foreground" aria-label={decision.window === "night" ? "Night" : "Day"} />
                      <span className="font-medium">{label}</span>
                      {decision.reserve_soc != null && decision.mode === "sbg" ? (
                        <span className="text-muted-foreground">until {Math.round(decision.reserve_soc)}%</span>
                      ) : null}
                      <span
                        className={cn(
                          "rounded-full px-1.5 py-px text-micro uppercase tracking-wide",
                          decision.applied ? "bg-primary/10 text-foreground" : "bg-muted text-muted-foreground",
                        )}
                      >
                        {outcome(decision)}
                      </span>
                      {decision.dry_run ? <span className="text-micro uppercase tracking-wide text-sky-600 dark:text-sky-400">dry run</span> : null}
                    </div>
                    <p className="text-sm leading-snug">{decision.reason}</p>
                    <p className="text-[11px] text-muted-foreground">
                      {decision.model ? `Decided by ${modelLabel(decision.model)}` : "Decided by the app's own rules"}
                    </p>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
        {canShowMore && decisions && decisions.length > 0 ? (
          <Button variant="ghost" size="sm" onClick={onShowMore}>
            Show more
          </Button>
        ) : null}
      </CardContent>
    </Card>
  );
}

import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AutomationConfig, AutomationInsights, AutomationStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { FlaskConical, RefreshCw, Settings } from "lucide-react";
import { BudgetBar } from "./BudgetBar";
import { automationPhaseLabel, clock, PHASE_VARIANTS, relativeUntil, Skeleton } from "./shared";

interface AutomationHeroProps {
  status: AutomationStatus;
  config: AutomationConfig;
  insights: AutomationInsights | undefined;
  saving: boolean;
  checking: boolean;
  onSave: (config: AutomationConfig) => void;
  onCheckNow: () => void;
  onOpenSettings: () => void;
}

function modeLine(status: AutomationStatus): string | null {
  const physical = status.mode_name;
  const effective = status.effective_mode;
  if (!physical && !effective) return null;
  const words = (mode: string | null) => (mode === "SBG" ? "on battery (SBG)" : mode === "Solar" ? "on grid (Solar)" : mode ?? "unknown");
  if (status.dry_run && effective && effective !== physical) {
    return `Would be ${words(effective)} · inverter is ${words(physical)}`;
  }
  return `Inverter ${words(physical ?? effective)}`;
}

function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

export function AutomationHero({
  status,
  config,
  insights,
  saving,
  checking,
  onSave,
  onCheckNow,
  onOpenSettings,
}: AutomationHeroProps) {
  const [confirmingLive, setConfirmingLive] = useState(false);
  const now = useNow(30_000);
  const blocked = status.phase === "blocked";
  const nextCheck = relativeUntil(status.next_check_at, now);
  const soc = insights?.simulated_soc ?? insights?.soc ?? null;
  const day = insights?.window === "day" ? insights.day : null;
  const headline = blocked
    ? status.blocked_reason ?? "Automation needs setup."
    : !status.enabled
      ? "Automation is off. Turn it on to let the agent plan tonight's battery use."
      : status.ai_reason ?? "Waiting for the first check…";

  const setDryRun = (value: boolean) => {
    if (!value) {
      setConfirmingLive(true);
      return;
    }
    onSave({ ...config, dry_run: true });
  };

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card">
      {status.dry_run && status.enabled ? (
        <div className="flex items-center gap-2 border-b border-sky-500/20 bg-sky-500/10 px-5 py-2 text-xs text-sky-700 dark:text-sky-300">
          <FlaskConical className="size-3.5" aria-hidden />
          Dry run — nothing is written to the inverter. Everything below shows what it would do.
        </div>
      ) : null}

      <div className="space-y-5 p-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={PHASE_VARIANTS[status.phase]}>{automationPhaseLabel(status.phase)}</Badge>
            {status.boost_until ? (
              <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium tabular-nums">
                Oven boost · on battery until {clock(status.boost_until)}
              </span>
            ) : null}
            {status.enabled && nextCheck && !blocked ? (
              <span className="text-xs text-muted-foreground tabular-nums">next check {nextCheck}</span>
            ) : null}
          </div>
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex items-center gap-2">
              <Label htmlFor="automation-enabled" className="text-xs text-muted-foreground">
                Enabled
              </Label>
              <Switch
                id="automation-enabled"
                checked={config.enabled}
                disabled={saving}
                onCheckedChange={(value) => onSave({ ...config, enabled: value })}
              />
            </div>
            <div className="flex items-center gap-2">
              <Label htmlFor="automation-dry-run" className="text-xs text-muted-foreground">
                Dry run
              </Label>
              <Switch id="automation-dry-run" checked={config.dry_run} disabled={saving} onCheckedChange={setDryRun} />
            </div>
            <Button variant="outline" size="sm" onClick={onCheckNow} disabled={checking || !status.enabled}>
              <RefreshCw className={cn(checking && "animate-spin motion-reduce:animate-none")} /> {checking ? "Checking…" : "Check now"}
            </Button>
          </div>
        </div>

        {confirmingLive ? (
          <div
            role="alertdialog"
            aria-label="Turn off dry run"
            className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm"
          >
            <p>
              <span className="font-semibold">Turn off dry run?</span> The app will switch your inverter between battery and grid for real.
            </p>
            <div className="flex gap-2">
              <Button size="sm" variant="outline" onClick={() => setConfirmingLive(false)}>
                Keep dry run
              </Button>
              <Button
                size="sm"
                onClick={() => {
                  setConfirmingLive(false);
                  onSave({ ...config, dry_run: false });
                }}
              >
                Switch for real
              </Button>
            </div>
          </div>
        ) : null}

        <div className="space-y-2">
          <p
            className={cn(
              "max-w-3xl text-xl font-medium leading-snug tracking-tight text-balance",
              blocked && "text-amber-700 dark:text-amber-300",
            )}
          >
            {headline}
          </p>
          <p className="text-sm text-muted-foreground">
            {[modeLine(status), status.last_event ? `last: ${status.last_event.message}` : null].filter(Boolean).join(" · ")}
          </p>
          {blocked ? (
            <Button size="sm" variant="outline" onClick={onOpenSettings}>
              <Settings /> Open Settings
            </Button>
          ) : null}
        </div>

        {insights == null ? (
          <Skeleton className="h-12 w-full" />
        ) : soc != null ? (
          <BudgetBar
            soc={soc}
            floor={insights.floor_soc}
            reserve={insights.reserve_soc}
            projected={day?.soc_at_sunset ?? null}
            projectedLabel={day ? `At sunset ≈ ${Math.round(day.soc_at_sunset)}%${day.full_at ? ` (full ${clock(day.full_at)})` : ""}` : undefined}
          />
        ) : null}
      </div>
    </section>
  );
}

import { automationPhaseLabel, PHASE_VARIANTS, relativeUntil } from "@/components/automation/shared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { AutomationStatus } from "@/lib/types";
import { ArrowRight, BrainCircuit } from "lucide-react";

function planLine(status: AutomationStatus): string | null {
  if (status.phase === "night_on_battery" || status.phase === "night_verifying") {
    return status.reserve_soc != null ? `On battery until ${Math.round(status.reserve_soc)}%` : "On battery";
  }
  if (status.phase === "night_reserve") return "Reserve kept — Solar mode for the rest of the night";
  if (status.phase === "day") {
    return status.effective_mode === "SBG" ? "SBG — the sun is charging the battery" : "Solar — grid covers the house, battery kept";
  }
  return null;
}

export function AutomationSummaryCard({ status, onOpen }: { status: AutomationStatus | undefined; onOpen: () => void }) {
  if (!status) {
    return <section className="h-full min-h-40 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }
  const next = relativeUntil(status.next_check_at);
  const plan = planLine(status);
  const text = !status.enabled
    ? "Off. Turn it on to let the agent plan tonight's battery use."
    : status.phase === "blocked"
      ? status.blocked_reason ?? "Needs setup."
      : status.ai_reason ?? "Waiting for the first check…";

  return (
    <section className="flex h-full flex-col gap-3 rounded-xl border border-border bg-card p-5">
      <div className="flex items-center justify-between gap-2">
        <h3 className="inline-flex items-center gap-2 text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
          <BrainCircuit className="size-4" aria-hidden /> Automation
        </h3>
        <div className="flex items-center gap-1.5">
          {status.enabled && status.dry_run ? (
            <span className="text-[10px] font-medium uppercase tracking-wide text-sky-600 dark:text-sky-400">dry run</span>
          ) : null}
          <Badge variant={PHASE_VARIANTS[status.phase]}>{automationPhaseLabel(status.phase)}</Badge>
        </div>
      </div>
      {plan && status.enabled ? <p className="text-lg font-semibold leading-tight tabular-nums">{plan}</p> : null}
      <p className="line-clamp-3 text-sm leading-snug text-muted-foreground">{text}</p>
      <div className="mt-auto flex items-center justify-between gap-2 pt-1">
        <span className="text-xs tabular-nums text-muted-foreground">{status.enabled && next ? `Next check ${next}` : ""}</span>
        <Button variant="ghost" size="sm" onClick={onOpen}>
          Open <ArrowRight />
        </Button>
      </div>
    </section>
  );
}

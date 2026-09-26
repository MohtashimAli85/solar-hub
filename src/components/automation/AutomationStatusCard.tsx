import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState, Progress, Skeleton } from "@/components/ui/switch";
import { fmtDateTime, fmtNumber, fmtTime } from "@/lib/format";
import type {
  AutomationEstimate,
  AutomationLiveStatus,
  AutomationLogEntry,
  AutomationPhase,
  AutomationStatus,
} from "@/lib/types";

interface AutomationStatusCardProps {
  status: AutomationStatus | undefined;
  limit?: number;
  isLoading?: boolean;
}

const PHASE_META: Record<
  AutomationPhase,
  { label: string; badge: "default" | "secondary" | "outline" | "success" | "warning" | "destructive" | "info" }
> = {
  day: { label: "Daytime", badge: "secondary" },
  waiting: { label: "Waiting", badge: "secondary" },
  probing: { label: "Probing", badge: "info" },
  holding: { label: "Holding SBG", badge: "success" },
  backed_off: { label: "Backed off", badge: "warning" },
  user_override: { label: "User override", badge: "default" },
  low_soc: { label: "Low SOC", badge: "destructive" },
};

export function automationPhaseLabel(phase: AutomationPhase | undefined): string {
  return phase ? PHASE_META[phase].label : "–";
}

function estimateText(a: number | null | undefined, h: number | null | undefined): string {
  if (a == null) return "no load data";
  return h == null ? `${fmtNumber(a)} A → indefinite` : `${fmtNumber(a)} A → ${fmtNumber(h)}h`;
}

function runtimePercent(runtime: number | null | undefined, required: number | null | undefined): number {
  if (runtime == null || required == null || required <= 0) return 0;
  return Math.min(100, (runtime / required) * 100);
}

function phaseSummary(status: AutomationStatus): string {
  const { phase, estimate, verified, required_h } = status;
  const needed = required_h != null ? `${fmtNumber(required_h)}h needed` : "";
  switch (phase) {
    case "day":
      return status.automation_engaged
        ? "Daytime — watching PV and discharge to switch back from SBG"
        : "Daytime — waiting for the night window";
    case "waiting": {
      if (estimate.discharge_a == null) {
        return "Entering the night window — no load estimate yet, probing SBG";
      }
      const covers =
        estimate.runtime_h == null || (required_h != null && estimate.runtime_h >= required_h);
      return `Estimated ${estimateText(estimate.discharge_a, estimate.runtime_h)}${
        needed ? `, ${needed}` : ""
      } — ${covers ? "switching to SBG" : "staying on Solar"}`;
    }
    case "probing":
      return `Probing real SBG discharge after a ${estimateText(
        estimate.discharge_a,
        estimate.runtime_h,
      )} estimate`;
    case "holding":
      return `Holding SBG — verified ${estimateText(
        verified.discharge_a,
        verified.runtime_h,
      )}${needed ? `, ${needed}` : ""} (tolerates small shortfalls at high SOC)`;
    case "backed_off":
      return "Backed off — SBG fell short on repeated checks; watching for the load to drop";
    case "user_override":
      return "You changed the output mode manually — automation is standing down";
    case "low_soc":
      return "Battery near the reserve — automation stopped for today";
  }
}

function LiveRow({ live }: { live: AutomationLiveStatus | undefined }) {
  if (!live) return null;
  const bits: string[] = [];
  if (live.soc != null) bits.push(`${fmtNumber(live.soc, 0)}% SOC`);
  if (live.pv_w != null) bits.push(`${fmtNumber(Math.round(live.pv_w), 0)} W PV`);
  if (live.load_w != null) bits.push(`${fmtNumber(Math.round(live.load_w), 0)} W load`);
  if (live.battery_current_a != null) bits.push(`${fmtNumber(Math.abs(live.battery_current_a))} A`);
  if (live.grid_on != null) bits.push(live.grid_on ? "grid on" : "grid off");
  if (live.smart_load != null) bits.push(`smart load ${live.smart_load === 1 ? "on" : "off"}`);
  if (bits.length === 0) return null;
  return (
    <p className="text-xs tabular-nums text-muted-foreground">
      <span className="text-foreground/90">live</span> · {bits.join(" · ")}
    </p>
  );
}

function MeasureRow({
  title,
  estimate,
  required,
  engaged,
}: {
  title: string;
  estimate: AutomationEstimate;
  required: number;
  engaged: boolean;
}) {
  if (estimate.discharge_a == null) return null;
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2 text-[13px] tabular-nums">
        <span className="text-muted-foreground">{title}</span>
        <span>{estimateText(estimate.discharge_a, estimate.runtime_h)}</span>
      </div>
      <Progress value={runtimePercent(estimate.runtime_h, required)} className={engaged ? "" : "opacity-40"} />
    </div>
  );
}

interface CollapsedLog {
  timestamp: string;
  message: string;
  count: number;
}

function collapseLogs(logs: AutomationLogEntry[]): CollapsedLog[] {
  const collapsed: CollapsedLog[] = [];
  for (const entry of logs) {
    const last = collapsed[collapsed.length - 1];
    if (last && last.message === entry.message) {
      last.count += 1;
    } else {
      collapsed.push({ timestamp: entry.timestamp, message: entry.message, count: 1 });
    }
  }
  return collapsed;
}

export function AutomationStatusCard({ status, limit, isLoading = false }: AutomationStatusCardProps) {
  const collapsed = collapseLogs(status?.logs ?? []);
  const visible = limit != null ? collapsed.slice(0, limit) : collapsed;
  const phase = status?.phase;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle>Automation</CardTitle>
          <div className="flex items-center gap-1.5">
            {status?.warning ? <Badge variant="destructive">⚠ Reverted</Badge> : null}
            {phase ? <Badge variant={PHASE_META[phase].badge}>{PHASE_META[phase].label}</Badge> : null}
            {isLoading ? (
              <Skeleton className="h-5 w-16" />
            ) : (
              <Badge variant={status?.enabled ? "success" : "secondary"}>
                {status?.enabled ? "Enabled" : "Disabled"}
              </Badge>
            )}
          </div>
        </div>
        <CardDescription>
          {isLoading ? (
            <Skeleton className="h-3 w-48" />
          ) : (
            <>
              Last check: {fmtDateTime(status?.last_check)} · next: {fmtTime(status?.next_check)}
              {status?.current_mode_name ? ` · Output: ${status.current_mode_name}` : ""}
            </>
          )}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {isLoading ? (
          <div className="space-y-3" aria-busy="true" aria-label="Loading automation status">
            <Skeleton className="h-5 w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ) : (
          <>
            {status && phase ? (
              <p className={status.automation_engaged ? "font-medium" : "text-muted-foreground"}>
                {phaseSummary(status)}
              </p>
            ) : null}
            <LiveRow live={status?.live} />
            {status && status.required_h != null ? (
              <div className="space-y-2 border-t border-border pt-3">
                <MeasureRow
                  title="Estimated"
                  estimate={status.estimate}
                  required={status.required_h}
                  engaged={status.automation_engaged}
                />
                <MeasureRow
                  title="Verified"
                  estimate={status.verified}
                  required={status.required_h}
                  engaged={status.automation_engaged}
                />
              </div>
            ) : null}
            <div className="border-t border-border pt-3">
              {visible.length === 0 ? (
                <EmptyState message="No activity yet — wait for the interval or hit Check now." />
              ) : (
                <ul className="space-y-1.5">
                  {visible.map((log, index) => (
                    <li key={`${log.timestamp}-${index}`} className="flex gap-2 text-sm">
                      <span className="shrink-0 tabular-nums text-muted-foreground">
                        {fmtTime(log.timestamp)}
                      </span>
                      <span className="min-w-0 flex-1">{log.message}</span>
                      {log.count > 1 ? (
                        <span className="shrink-0 tabular-nums text-muted-foreground">×{log.count}</span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
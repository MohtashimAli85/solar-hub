import type { ReactNode } from "react";
import { fmtDuration } from "@/lib/format";
import type { AutomationPhase, SmartLoadInsight } from "@/lib/types";
import { cn } from "@/lib/utils";

const PHASE_LABELS: Record<AutomationPhase, string> = {
  idle: "Off",
  day: "Day check",
  night_deciding: "Night · planning",
  night_verifying: "Night · measuring draw",
  night_on_battery: "Night · on battery",
  night_reserve: "Night · reserve kept",
  paused: "Paused",
  blocked: "Needs setup",
};

export const PHASE_VARIANTS: Record<AutomationPhase, "secondary" | "info" | "success" | "warning" | "destructive"> = {
  idle: "secondary",
  day: "info",
  night_deciding: "info",
  night_verifying: "info",
  night_on_battery: "success",
  night_reserve: "success",
  paused: "warning",
  blocked: "destructive",
};

export function automationPhaseLabel(phase: AutomationPhase): string {
  return PHASE_LABELS[phase] ?? phase;
}

export const VIZ = {
  accent: "var(--viz-accent)",
  context: "var(--viz-context)",
  contextStrong: "var(--viz-context-strong)",
  grid: "var(--viz-grid)",
  ink: "var(--muted-foreground)",
};

export function toMs(local: string | null | undefined): number | null {
  if (!local) return null;
  const ms = new Date(local).getTime();
  return Number.isNaN(ms) ? null : ms;
}

export function clock(local: string | number | null | undefined): string {
  if (local == null) return "–";
  const date = new Date(local);
  if (Number.isNaN(date.getTime())) return "–";
  return date.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

export function hourLabel(hour: number): string {
  const suffix = hour < 12 ? "AM" : "PM";
  const twelve = hour % 12 === 0 ? 12 : hour % 12;
  return `${twelve} ${suffix}`;
}

export function dayLabel(date: string): string {
  const parsed = new Date(`${date}T12:00:00`);
  if (Number.isNaN(parsed.getTime())) return date;
  return parsed.toLocaleDateString("en-US", { weekday: "short" });
}

export function minutesText(minutes: number): string {
  return fmtDuration(minutes);
}

export function relativeUntil(local: string | null | undefined, now = Date.now()): string | null {
  const target = toMs(local);
  if (target == null) return null;
  const minutes = Math.round((target - now) / 60_000);
  if (minutes <= 0) return "now";
  return `in ${minutesText(minutes)}`;
}

export function SectionTitle({ eyebrow, title, aside }: { eyebrow?: string; title: string; aside?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-2">
      <div className="space-y-0.5">
        {eyebrow ? (
          <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">{eyebrow}</p>
        ) : null}
        <h3 className="text-base font-semibold tracking-tight">{title}</h3>
      </div>
      {aside}
    </div>
  );
}

export function Stat({ label, value, hint, className }: { label: string; value: ReactNode; hint?: ReactNode; className?: string }) {
  return (
    <div className={cn("min-w-0 space-y-0.5", className)}>
      <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="text-lg font-semibold leading-tight tabular-nums">{value}</p>
      {hint ? <p className="text-[11px] leading-snug text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

export function Chip({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "accent" }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs tabular-nums",
        tone === "accent" ? "border-primary/30 bg-primary/10 text-foreground" : "border-border bg-muted/40 text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}

export function EmptyNote({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
      {children}
    </div>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("animate-pulse rounded-md bg-muted motion-reduce:animate-none", className)} />;
}

export function ChartTooltipBox({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-md border border-border bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-md tabular-nums">
      {children}
    </div>
  );
}

export function smartLoadText(smart: SmartLoadInsight | null | undefined, window: "day" | "night" | null): string | null {
  if (!smart) return null;
  const state = smart.on == null ? "" : smart.on ? " · now on" : " · now off";
  if (window === "night") {
    if (smart.done) return `Smart load on for the night${state}`;
    return `Smart load on at ${clock(smart.planned_on_at)} (${smart.season})${state}`;
  }
  const tonight = smart.season === "summer" ? `on at ${clock(smart.night_on_at)} tonight` : `on between ${clock(smart.night_on_at)} and 11 PM tonight`;
  return `Smart load off for the day, ${tonight}${state}`;
}

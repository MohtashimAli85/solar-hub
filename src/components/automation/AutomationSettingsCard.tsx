import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input, Label } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useAutomation } from "@/hooks/useAutomation";
import type { AutomationConfig, AutomationPhase } from "@/lib/types";
import { cn } from "@/lib/utils";
import { RefreshCw } from "lucide-react";

const PHASE_LABELS: Record<AutomationPhase, string> = {
  idle: "Idle",
  night_deciding: "Night · deciding",
  night_verifying: "Night · verifying",
  night_holding: "Night · holding",
  paused: "Paused",
  morning: "Morning",
  blocked: "Blocked",
};

const PHASE_VARIANTS: Record<AutomationPhase, "secondary" | "info" | "success" | "warning" | "destructive"> = {
  idle: "secondary",
  night_deciding: "info",
  night_verifying: "info",
  night_holding: "success",
  paused: "warning",
  morning: "info",
  blocked: "destructive",
};

export function automationPhaseLabel(phase: AutomationPhase): string {
  return PHASE_LABELS[phase] ?? phase;
}

function fmtTime(iso: string | null): string {
  if (!iso) return "–";
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

interface Range {
  min: number;
  max: number;
}

const RANGES: Record<keyof Omit<AutomationConfig, "enabled" | "dry_run" | "notifications_enabled">, Range> = {
  check_interval_minutes: { min: 1, max: 240 },
  min_soc_percent: { min: 0, max: 100 },
  capacity_ah: { min: 1, max: 10_000 },
  sunrise_buffer_hours: { min: 0, max: 6 },
  morning_window_hours: { min: 0.5, max: 12 },
  morning_charge_threshold_a: { min: 0, max: 200 },
  pv_array_watts: { min: 0, max: 100_000 },
};

function fieldError(key: keyof typeof RANGES, value: number): string | null {
  const range = RANGES[key];
  if (Number.isNaN(value)) return "Enter a number";
  if (value < range.min || value > range.max) return `${range.min}–${range.max}`;
  return null;
}

interface FieldProps {
  label: string;
  help: string;
  value: number;
  step?: number;
  error: string | null;
  onChange: (value: number) => void;
}

function NumberField({ label, help, value, step = 1, error, onChange }: FieldProps) {
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Input
        type="number"
        step={step}
        value={Number.isNaN(value) ? "" : value}
        onChange={(event) => onChange(event.target.valueAsNumber)}
        className={cn(error && "border-destructive focus-visible:ring-destructive")}
      />
      <p className={cn("text-[11px] leading-snug", error ? "text-destructive" : "text-muted-foreground")}>
        {error ?? help}
      </p>
    </div>
  );
}

export function AutomationSettingsCard() {
  const { status, config, saveConfig, checkNow, testNotification } = useAutomation();
  const [form, setForm] = useState<AutomationConfig | null>(null);

  useEffect(() => {
    if (config && !form) setForm(config);
  }, [config, form]);

  if (!form || !status) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Automation</CardTitle>
          <CardDescription>Loading…</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  const set = <K extends keyof AutomationConfig>(key: K, value: AutomationConfig[K]) =>
    setForm((previous) => (previous ? { ...previous, [key]: value } : previous));

  const errors = Object.fromEntries(
    (Object.keys(RANGES) as (keyof typeof RANGES)[]).map((key) => [key, fieldError(key, form[key])]),
  ) as Record<keyof typeof RANGES, string | null>;
  const hasErrors = Object.values(errors).some(Boolean);
  const dirty = config ? JSON.stringify(config) !== JSON.stringify(form) : false;

  const save = () => {
    if (hasErrors) return;
    saveConfig.mutate(form);
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <CardTitle>Automation</CardTitle>
            <CardDescription>Gemini-driven night SBG switching and a morning charge-current check.</CardDescription>
          </div>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2">
              <Label className="text-xs text-muted-foreground">Enabled</Label>
              <Switch checked={form.enabled} onCheckedChange={(value) => set("enabled", value)} />
            </div>
            <div className="flex items-center gap-2">
              <Label className="text-xs text-muted-foreground">Dry run</Label>
              <Switch checked={form.dry_run} onCheckedChange={(value) => set("dry_run", value)} />
            </div>
            <Badge variant={PHASE_VARIANTS[status.phase]}>{automationPhaseLabel(status.phase)}</Badge>
          </div>
        </div>
        {status.blocked_reason ? (
          <p className="text-[11px] text-amber-600 dark:text-amber-400">{status.blocked_reason}</p>
        ) : null}
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="flex flex-wrap gap-x-5 gap-y-1 rounded-lg border border-border bg-muted/40 px-3 py-2 text-[13px] text-muted-foreground">
          <span>{status.last_event?.message ?? "No events yet"}</span>
          <span>
            sunset <span className="text-foreground">{fmtTime(status.sunset)}</span>
          </span>
          <span>
            sunrise <span className="text-foreground">{fmtTime(status.sunrise)}</span>
          </span>
        </div>

        <div className="grid gap-5 sm:grid-cols-2">
          <div className="space-y-3">
            <h4 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">Night</h4>
            <NumberField
              label="Sunrise buffer (hours)"
              help="Added on top of the hours until sunrise for PV ramp-up."
              value={form.sunrise_buffer_hours}
              step={0.5}
              error={errors.sunrise_buffer_hours}
              onChange={(value) => set("sunrise_buffer_hours", value)}
            />
          </div>

          <div className="space-y-3">
            <h4 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">Morning</h4>
            <NumberField
              label="Window (hours)"
              help="How long after sunrise the morning check runs."
              value={form.morning_window_hours}
              step={0.5}
              error={errors.morning_window_hours}
              onChange={(value) => set("morning_window_hours", value)}
            />
            <NumberField
              label="Charge threshold (A)"
              help="Above this charge current, use SBG."
              value={form.morning_charge_threshold_a}
              error={errors.morning_charge_threshold_a}
              onChange={(value) => set("morning_charge_threshold_a", value)}
            />
            <NumberField
              label="PV array (W)"
              help="Array size, for the expected clear-sky PV. 0 = unknown."
              value={form.pv_array_watts}
              error={errors.pv_array_watts}
              onChange={(value) => set("pv_array_watts", value)}
            />
          </div>

          <div className="space-y-3">
            <h4 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">Battery</h4>
            <NumberField
              label="Floor (%)"
              help="Automation pauses for the night below this SOC."
              value={form.min_soc_percent}
              error={errors.min_soc_percent}
              onChange={(value) => set("min_soc_percent", value)}
            />
            <NumberField
              label="Fallback capacity (Ah)"
              help="Used only when there's no BMS and no inverter capacity field."
              value={form.capacity_ah}
              error={errors.capacity_ah}
              onChange={(value) => set("capacity_ah", value)}
            />
          </div>

          <div className="space-y-3">
            <h4 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">General</h4>
            <NumberField
              label="Check interval (minutes)"
              help="Normal tick outside of verifying."
              value={form.check_interval_minutes}
              error={errors.check_interval_minutes}
              onChange={(value) => set("check_interval_minutes", value)}
            />
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
              <div>
                <p className="text-sm font-medium">Notifications</p>
                <p className="text-[11px] text-muted-foreground">Gates all banners, battery alerts included.</p>
              </div>
              <Switch
                checked={form.notifications_enabled}
                onCheckedChange={(value) => set("notifications_enabled", value)}
              />
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <Button onClick={save} disabled={!dirty || hasErrors || saveConfig.isPending}>
            {saveConfig.isPending ? "Saving…" : "Save"}
          </Button>
          <Button variant="outline" onClick={() => checkNow.mutate()} disabled={checkNow.isPending}>
            <RefreshCw className={cn(checkNow.isPending && "animate-spin")} /> Check now
          </Button>
          <Button variant="outline" onClick={() => testNotification.mutate()} disabled={testNotification.isPending}>
            Send test
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

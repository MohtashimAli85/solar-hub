import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input, Label } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Bell, Play, Save } from "lucide-react";
import { useEffect, useState } from "react";
import type { AutomationConfig } from "@/lib/types";

interface AutomationConfigFormProps {
  config: AutomationConfig | undefined;
  saving: boolean;
  checking: boolean;
  testing: boolean;
  onSave: (config: AutomationConfig) => void;
  onCheckNow: () => void;
  onTestNotification: () => void;
}

function hourOptions() {
  return Array.from({ length: 24 }, (_, hour) => (
    <option key={hour} value={hour}>
      {String(hour).padStart(2, "0")}:00
    </option>
  ));
}

export function AutomationConfigForm({
  config,
  saving,
  checking,
  testing,
  onSave,
  onCheckNow,
  onTestNotification,
}: AutomationConfigFormProps) {
  const [form, setForm] = useState<AutomationConfig | null>(null);

  useEffect(() => {
    if (config && !form) {
      setForm({
        ...config,
        probe_required_samples: config.probe_required_samples ?? 3,
        min_hold_minutes: config.min_hold_minutes ?? 30,
        deficit_tolerance_hours: config.deficit_tolerance_hours ?? 2,
        high_soc_hold_percent: config.high_soc_hold_percent ?? 85,
        hold_failures_before_revert: config.hold_failures_before_revert ?? 2,
      });
    }
  }, [config, form]);

  if (!form) {
    return <p className="text-sm text-muted-foreground">Loading configuration…</p>;
  }

  const set = (patch: Partial<AutomationConfig>) => setForm((previous) => ({ ...previous!, ...patch }));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Night SBG automation</CardTitle>
        <CardDescription>
          Switch to SBG once, verify over several samples, then hold through small
          shortfalls at high SOC — revert only after repeated failures or low SOC.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="flex items-center justify-between gap-4">
          <div>
            <Label>Enabled</Label>
            <p className="text-sm text-muted-foreground">
              Runs in the background while the app is open.
            </p>
          </div>
          <Switch checked={form.enabled} onCheckedChange={(enabled) => set({ enabled })} />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <Label>Check interval (minutes)</Label>
            <Input
              type="number"
              min={1}
              max={240}
              value={form.check_interval_minutes}
              onChange={(event) => set({ check_interval_minutes: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Target hour (revert by)</Label>
            <Input
              type="number"
              min={0}
              max={23}
              value={form.target_hour}
              onChange={(event) => set({ target_hour: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Safety margin (hours)</Label>
            <Input
              type="number"
              min={0}
              max={24}
              step={0.5}
              value={form.safety_margin_hours}
              onChange={(event) => set({ safety_margin_hours: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Window start</Label>
            <select
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              value={form.window_start_hour}
              onChange={(event) => set({ window_start_hour: Number(event.target.value) })}
            >
              {hourOptions()}
            </select>
          </div>
          <div className="space-y-1.5">
            <Label>Window end</Label>
            <select
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              value={form.window_end_hour}
              onChange={(event) => set({ window_end_hour: Number(event.target.value) })}
            >
              {hourOptions()}
            </select>
          </div>
          <div className="space-y-1.5">
            <Label>Minimum SOC (%)</Label>
            <Input
              type="number"
              min={0}
              max={100}
              value={form.min_soc_percent}
              onChange={(event) => set({ min_soc_percent: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Reserve SOC (%)</Label>
            <Input
              type="number"
              min={0}
              max={100}
              value={form.reserve_soc_percent}
              onChange={(event) => set({ reserve_soc_percent: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Battery capacity (Ah)</Label>
            <Input
              type="number"
              min={1}
              value={form.capacity_ah}
              onChange={(event) => set({ capacity_ah: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Probe samples (checks)</Label>
            <Input
              type="number"
              min={1}
              max={12}
              value={form.probe_required_samples}
              onChange={(event) => set({ probe_required_samples: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Min hold (minutes)</Label>
            <Input
              type="number"
              min={0}
              max={240}
              value={form.min_hold_minutes}
              onChange={(event) => set({ min_hold_minutes: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Deficit tolerance (hours)</Label>
            <Input
              type="number"
              min={0}
              max={12}
              step={0.5}
              value={form.deficit_tolerance_hours}
              onChange={(event) => set({ deficit_tolerance_hours: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>High SOC hold (%)</Label>
            <Input
              type="number"
              min={0}
              max={100}
              value={form.high_soc_hold_percent}
              onChange={(event) => set({ high_soc_hold_percent: Number(event.target.value) })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Failures before revert</Label>
            <Input
              type="number"
              min={1}
              max={10}
              value={form.hold_failures_before_revert}
              onChange={(event) => set({ hold_failures_before_revert: Number(event.target.value) })}
            />
          </div>
        </div>

        <div className="space-y-3 border-t border-border pt-4">
          <div>
            <Label>Daytime switch-back</Label>
            <p className="text-sm text-muted-foreground">
              Once PV drops below this and the battery is draining, switch the inverter back
              from SBG to Solar.
            </p>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>PV below (W)</Label>
              <Input
                type="number"
                min={0}
                value={form.day_pv_threshold_watts}
                onChange={(event) => set({ day_pv_threshold_watts: Number(event.target.value) })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Discharge above (A)</Label>
              <Input
                type="number"
                min={0}
                step={0.5}
                value={form.day_discharge_threshold_a}
                onChange={(event) => set({ day_discharge_threshold_a: Number(event.target.value) })}
              />
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between gap-4 border-t border-border pt-4">
          <div>
            <Label>Notifications</Label>
            <p className="text-sm text-muted-foreground">
              Desktop alerts for battery, grid and automation events.
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button variant="outline" size="sm" onClick={onTestNotification} disabled={testing}>
              <Bell />
              {testing ? "Sending…" : "Send test"}
            </Button>
            <Switch
              checked={form.notifications_enabled}
              onCheckedChange={(enabled) => set({ notifications_enabled: enabled })}
            />
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button onClick={() => onSave(form)} disabled={saving}>
            <Save />
            {saving ? "Saving…" : "Save configuration"}
          </Button>
          <Button variant="outline" onClick={onCheckNow} disabled={checking || !form.enabled}>
            <Play />
            Check now
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
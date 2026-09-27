import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input, Label, Select } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AutomationConfig } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ChevronDown } from "lucide-react";
import { hourLabel, SectionTitle } from "./shared";

type NumericKey = "check_interval_minutes" | "min_soc_percent" | "capacity_ah" | "sunrise_buffer_hours" | "pv_array_watts" | "oven_boost_amps";
type TuningKey = NumericKey | "night_start_hour" | "notifications_enabled";
type Tuning = Pick<AutomationConfig, TuningKey>;

const RANGES: Record<NumericKey, { min: number; max: number }> = {
  check_interval_minutes: { min: 1, max: 240 },
  min_soc_percent: { min: 0, max: 100 },
  capacity_ah: { min: 1, max: 10_000 },
  sunrise_buffer_hours: { min: 0, max: 6 },
  pv_array_watts: { min: 0, max: 100_000 },
  oven_boost_amps: { min: 0, max: 300 },
};

const NIGHT_START_HOURS = [17, 18, 19, 20, 21, 22, 23];

function fieldError(key: NumericKey, value: number): string | null {
  const range = RANGES[key];
  if (Number.isNaN(value)) return "Enter a number";
  if (value < range.min || value > range.max) return `${range.min}–${range.max}`;
  return null;
}

function pickTuning(config: AutomationConfig): Tuning {
  return {
    check_interval_minutes: config.check_interval_minutes,
    min_soc_percent: config.min_soc_percent,
    capacity_ah: config.capacity_ah,
    sunrise_buffer_hours: config.sunrise_buffer_hours,
    pv_array_watts: config.pv_array_watts,
    oven_boost_amps: config.oven_boost_amps,
    night_start_hour: config.night_start_hour,
    notifications_enabled: config.notifications_enabled,
  };
}

interface FieldProps {
  id: string;
  label: string;
  help: string;
  value: number;
  step?: number;
  error: string | null;
  onChange: (value: number) => void;
}

function NumberField({ id, label, help, value, step = 1, error, onChange }: FieldProps) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type="number"
        step={step}
        value={Number.isNaN(value) ? "" : value}
        onChange={(event) => onChange(event.target.valueAsNumber)}
        aria-invalid={error != null}
        className={cn(error && "border-destructive focus-visible:ring-destructive")}
      />
      <p className={cn("text-[11px] leading-snug", error ? "text-destructive" : "text-muted-foreground")}>{error ?? help}</p>
    </div>
  );
}

interface AutomationSettingsFormProps {
  config: AutomationConfig;
  saving: boolean;
  testing: boolean;
  onSave: (config: AutomationConfig) => void;
  onTestNotification: () => void;
}

export function AutomationSettingsForm({ config, saving, testing, onSave, onTestNotification }: AutomationSettingsFormProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<Tuning | null>(null);
  const saved = pickTuning(config);
  const form = draft ?? saved;
  const set = <K extends TuningKey>(key: K, value: Tuning[K]) => setDraft({ ...form, [key]: value });

  const errors = Object.fromEntries(
    (Object.keys(RANGES) as NumericKey[]).map((key) => [key, fieldError(key, form[key])]),
  ) as Record<NumericKey, string | null>;
  const hasErrors = Object.values(errors).some(Boolean);
  const dirty = draft != null && JSON.stringify(draft) !== JSON.stringify(saved);

  const save = () => {
    if (hasErrors || !draft) return;
    onSave({ ...config, ...draft });
    setDraft(null);
  };

  return (
    <Card>
      <CardHeader>
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          className="flex w-full items-center justify-between gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md"
        >
          <SectionTitle eyebrow="Tuning" title="When nights start, the floor and your battery" />
          <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} />
        </button>
      </CardHeader>
      {open ? (
        <CardContent className="space-y-5">
          <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
            <div className="space-y-1.5">
              <Label htmlFor="night-start">Night starts at</Label>
              <Select
                id="night-start"
                value={form.night_start_hour}
                onChange={(event) => set("night_start_hour", Number(event.target.value))}
              >
                {NIGHT_START_HOURS.map((hour) => (
                  <option key={hour} value={hour}>
                    {hourLabel(hour)}
                  </option>
                ))}
              </Select>
              <p className="text-[11px] leading-snug text-muted-foreground">
                Before this the house stays on grid; the battery plan starts here.
              </p>
            </div>
            <NumberField
              id="sunrise-buffer"
              label="Sunrise buffer (hours)"
              help="Extra time after sunrise before the panels can carry the house."
              value={form.sunrise_buffer_hours}
              step={0.5}
              error={errors.sunrise_buffer_hours}
              onChange={(value) => set("sunrise_buffer_hours", value)}
            />
            <NumberField
              id="floor"
              label="Floor (%)"
              help="Hard minimum. The agent's reserve never goes below this."
              value={form.min_soc_percent}
              error={errors.min_soc_percent}
              onChange={(value) => set("min_soc_percent", value)}
            />
            <NumberField
              id="pv-array"
              label="PV array (W)"
              help="Panel size, for sharper sun projections. 0 = use the charge current instead."
              value={form.pv_array_watts}
              error={errors.pv_array_watts}
              onChange={(value) => set("pv_array_watts", value)}
            />
            <NumberField
              id="capacity"
              label="Fallback capacity (Ah)"
              help="Only used when the battery (BMS) isn't connected."
              value={form.capacity_ah}
              error={errors.capacity_ah}
              onChange={(value) => set("capacity_ah", value)}
            />
            <NumberField
              id="oven-boost"
              label="Oven boost above (A)"
              help="At night on grid with no sun, a load this big (in battery amps) runs from the battery for 3 min with smart load off, then back to Solar. Up to 4 a night, never below tonight's reserve. 0 = off."
              value={form.oven_boost_amps}
              error={errors.oven_boost_amps}
              onChange={(value) => set("oven_boost_amps", value)}
            />
            <NumberField
              id="interval"
              label="Check interval (minutes)"
              help="How often the app looks when the agent hasn't asked for a sooner check."
              value={form.check_interval_minutes}
              error={errors.check_interval_minutes}
              onChange={(value) => set("check_interval_minutes", value)}
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
            <div>
              <Label htmlFor="notifications">Notifications</Label>
              <p className="text-[11px] text-muted-foreground">Banners for every switch, plus battery and grid alerts.</p>
            </div>
            <div className="flex items-center gap-3">
              <Button variant="ghost" size="sm" onClick={onTestNotification} disabled={testing}>
                Send test
              </Button>
              <Switch
                id="notifications"
                checked={form.notifications_enabled}
                onCheckedChange={(value) => set("notifications_enabled", value)}
              />
            </div>
          </div>

          {dirty ? (
            <div className="sticky bottom-0 -mx-5 -mb-5 flex items-center justify-between gap-3 rounded-b-lg border-t border-border bg-card/95 px-5 py-3 backdrop-blur">
              <p className="text-sm text-muted-foreground">{hasErrors ? "Fix the highlighted fields to save." : "You have unsaved changes."}</p>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={() => setDraft(null)}>
                  Discard
                </Button>
                <Button size="sm" onClick={save} disabled={hasErrors || saving}>
                  {saving ? "Saving…" : "Save changes"}
                </Button>
              </div>
            </div>
          ) : null}
        </CardContent>
      ) : null}
    </Card>
  );
}

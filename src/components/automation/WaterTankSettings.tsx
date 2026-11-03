import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input, Label, Select } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AutomationConfig } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ChevronDown } from "lucide-react";
import { hourLabel, SectionTitle } from "./shared";

type NumericKey = "water_motor_watts" | "water_full_soc" | "water_cloudy_soc" | "water_repeat_minutes" | "water_run_minutes";
type HourKey = "water_start_hour" | "water_end_hour" | "water_cloudy_hour";
type WaterKey = NumericKey | HourKey | "water_reminder_enabled";
type WaterForm = Pick<AutomationConfig, WaterKey>;

const RANGES: Record<NumericKey, { min: number; max: number }> = {
  water_motor_watts: { min: 100, max: 10_000 },
  water_full_soc: { min: 0, max: 100 },
  water_cloudy_soc: { min: 0, max: 100 },
  water_repeat_minutes: { min: 5, max: 120 },
  water_run_minutes: { min: 5, max: 240 },
};

const HOURS = Array.from({ length: 13 }, (_, index) => index + 6);

export function rangeError(range: { min: number; max: number }, value: number): string | null {
  if (Number.isNaN(value)) return "Enter a number";
  if (value < range.min || value > range.max) return `${range.min}–${range.max}`;
  return null;
}

function fieldError(key: NumericKey, value: number): string | null {
  return rangeError(RANGES[key], value);
}

function hoursError(form: WaterForm): string | null {
  if (form.water_end_hour <= form.water_start_hour) return "End must be after start";
  if (form.water_cloudy_hour < form.water_start_hour || form.water_cloudy_hour >= form.water_end_hour) {
    return "Weak-sun check must be inside the window";
  }
  return null;
}

function pickWater(config: AutomationConfig): WaterForm {
  return {
    water_reminder_enabled: config.water_reminder_enabled,
    water_motor_watts: config.water_motor_watts,
    water_start_hour: config.water_start_hour,
    water_end_hour: config.water_end_hour,
    water_full_soc: config.water_full_soc,
    water_cloudy_hour: config.water_cloudy_hour,
    water_cloudy_soc: config.water_cloudy_soc,
    water_repeat_minutes: config.water_repeat_minutes,
    water_run_minutes: config.water_run_minutes,
  };
}

export function NumberField({
  id,
  label,
  help,
  value,
  error,
  onChange,
}: {
  id: string;
  label: string;
  help: string;
  value: number;
  error: string | null;
  onChange: (value: number) => void;
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type="number"
        value={Number.isNaN(value) ? "" : value}
        onChange={(event) => onChange(event.target.valueAsNumber)}
        aria-invalid={error != null}
      />
      <p className={cn("text-micro leading-snug", error ? "text-destructive" : "text-muted-foreground")}>{error ?? help}</p>
    </div>
  );
}

export function HourField({
  id,
  label,
  help,
  value,
  onChange,
}: {
  id: string;
  label: string;
  help: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Select id={id} value={value} onChange={(event) => onChange(Number(event.target.value))}>
        {HOURS.map((hour) => (
          <option key={hour} value={hour}>
            {hourLabel(hour)}
          </option>
        ))}
      </Select>
      <p className="text-micro leading-snug text-muted-foreground">{help}</p>
    </div>
  );
}

interface WaterTankSettingsProps {
  config: AutomationConfig;
  saving: boolean;
  onSave: (config: AutomationConfig) => void;
}

export function WaterTankSettings({ config, saving, onSave }: WaterTankSettingsProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<WaterForm | null>(null);
  const saved = pickWater(config);
  const form = draft ?? saved;
  const set = <K extends WaterKey>(key: K, value: WaterForm[K]) => setDraft({ ...form, [key]: value });

  const errors = Object.fromEntries(
    (Object.keys(RANGES) as NumericKey[]).map((key) => [key, fieldError(key, form[key])]),
  ) as Record<NumericKey, string | null>;
  const hoursProblem = hoursError(form);
  const hasErrors = Object.values(errors).some(Boolean) || hoursProblem != null;
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
          <SectionTitle eyebrow="Water tank" title="When to remind you to run the motor" />
          <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} />
        </button>
      </CardHeader>
      {open ? (
        <CardContent className="space-y-5">
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
            <div>
              <Label htmlFor="water-enabled">Motor reminders</Label>
              <p className="text-micro text-muted-foreground">
                A banner every few minutes during good solar until you tap Done, plus one note if the window ends without it.
              </p>
            </div>
            <Switch
              id="water-enabled"
              checked={form.water_reminder_enabled}
              onCheckedChange={(value) => set("water_reminder_enabled", value)}
            />
          </div>

          <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
            <HourField
              id="water-start"
              label="From"
              help="No reminders before this."
              value={form.water_start_hour}
              onChange={(value) => set("water_start_hour", value)}
            />
            <HourField
              id="water-end"
              label="Until"
              help="At this time, one 'tank not filled' note if you haven't tapped Done."
              value={form.water_end_hour}
              onChange={(value) => set("water_end_hour", value)}
            />
            <NumberField
              id="water-motor"
              label="Motor (W)"
              help="Spare solar needed to run it. With the battery full this comes from the forecast and PV array size, since the inverter holds solar down to the load."
              value={form.water_motor_watts}
              error={errors.water_motor_watts}
              onChange={(value) => set("water_motor_watts", value)}
            />
            <NumberField
              id="water-full-soc"
              label="Battery full at (%)"
              help="At or above this, the forecast decides. Below it, the live spare solar has to reach the motor's watts."
              value={form.water_full_soc}
              error={errors.water_full_soc}
              onChange={(value) => set("water_full_soc", value)}
            />
            <HourField
              id="water-cloudy-hour"
              label="Weak sun if not full by"
              help="If the battery isn't full by now, it's a weak-sun day: remind at the sunniest hour left."
              value={form.water_cloudy_hour}
              onChange={(value) => set("water_cloudy_hour", value)}
            />
            <NumberField
              id="water-cloudy-soc"
              label="Weak-sun battery (%)"
              help="On a weak-sun day, only remind with at least this much battery."
              value={form.water_cloudy_soc}
              error={errors.water_cloudy_soc}
              onChange={(value) => set("water_cloudy_soc", value)}
            />
            <NumberField
              id="water-run"
              label="Motor runs for (minutes)"
              help="How long the tank takes to fill, so the geyser and scooter plans leave room for it."
              value={form.water_run_minutes}
              error={errors.water_run_minutes}
              onChange={(value) => set("water_run_minutes", value)}
            />
            <NumberField
              id="water-repeat"
              label="Repeat every (minutes)"
              help="How often to remind while conditions stay good."
              value={form.water_repeat_minutes}
              error={errors.water_repeat_minutes}
              onChange={(value) => set("water_repeat_minutes", value)}
            />
          </div>

          {hoursProblem ? <p className="text-sm text-destructive">{hoursProblem}</p> : null}

          {dirty ? (
            <div className="sticky bottom-0 -mx-4 -mb-4 sm:-mx-5 sm:-mb-5 flex items-center justify-between gap-3 rounded-b-lg border-t border-border bg-card/95 px-4 sm:px-5 py-3 backdrop-blur">
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

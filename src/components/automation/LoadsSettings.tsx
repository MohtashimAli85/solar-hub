import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input, Label } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { getGeyserRemote, pressGeyserButton, readScooterNow } from "@/lib/tauri";
import type { AutomationConfig, GeyserButton } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ChevronDown } from "lucide-react";
import { clock, SectionTitle } from "./shared";
import { HourField, NumberField, rangeError } from "./WaterTankSettings";

type Range = { min: number; max: number };

function useDraft<T extends object>(saved: T) {
  const [draft, setDraft] = useState<T | null>(null);
  const form = draft ?? saved;
  return {
    form,
    set: <K extends keyof T>(key: K, value: T[K]) => setDraft({ ...form, [key]: value }),
    dirty: draft != null && JSON.stringify(draft) !== JSON.stringify(saved),
    draft,
    reset: () => setDraft(null),
  };
}

function SettingsCard({
  eyebrow,
  title,
  dirty,
  hasErrors,
  saving,
  onDiscard,
  onSave,
  children,
}: {
  eyebrow: string;
  title: string;
  dirty: boolean;
  hasErrors: boolean;
  saving: boolean;
  onDiscard: () => void;
  onSave: () => void;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Card>
      <CardHeader>
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          className="flex w-full items-center justify-between gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md"
        >
          <SectionTitle eyebrow={eyebrow} title={title} />
          <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} />
        </button>
      </CardHeader>
      {open ? (
        <CardContent className="space-y-5">
          {children}
          {dirty ? (
            <div className="sticky bottom-0 -mx-4 -mb-4 sm:-mx-5 sm:-mb-5 flex items-center justify-between gap-3 rounded-b-lg border-t border-border bg-card/95 px-4 sm:px-5 py-3 backdrop-blur">
              <p className="text-sm text-muted-foreground">
                {hasErrors ? "Fix the highlighted fields to save." : "You have unsaved changes."}
              </p>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={onDiscard}>
                  Discard
                </Button>
                <Button size="sm" onClick={onSave} disabled={hasErrors || saving}>
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

function SwitchRow({
  id,
  label,
  help,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  help: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
      <div>
        <Label htmlFor={id}>{label}</Label>
        <p className="text-micro text-muted-foreground">{help}</p>
      </div>
      <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} />
    </div>
  );
}

function TextField({
  id,
  label,
  help,
  value,
  placeholder,
  onChange,
}: {
  id: string;
  label: string;
  help: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input id={id} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />
      <p className="text-micro leading-snug text-muted-foreground">{help}</p>
    </div>
  );
}

interface SettingsProps {
  config: AutomationConfig;
  saving: boolean;
  onSave: (config: AutomationConfig) => void;
}

type GeyserForm = Pick<
  AutomationConfig,
  | "geyser_enabled"
  | "geyser_start_hour"
  | "geyser_end_hour"
  | "geyser_low_watts"
  | "geyser_high_watts"
  | "geyser_daily_kwh"
  | "geyser_tank_litres"
  | "geyser_winter_only"
  | "inverter_max_w"
  | "geyser_esp32_host"
  | "geyser_auto_control"
  | "geyser_first_weekdays"
  | "geyser_first_below_c"
>;

const GEYSER_RANGES = {
  geyser_low_watts: { min: 100, max: 5_000 },
  geyser_high_watts: { min: 100, max: 5_000 },
  geyser_daily_kwh: { min: 0.5, max: 20 },
  geyser_tank_litres: { min: 10, max: 500 },
  inverter_max_w: { min: 1_000, max: 20_000 },
  geyser_first_below_c: { min: -20, max: 40 },
} satisfies Record<string, Range>;

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const REMOTE_BUTTONS: { name: GeyserButton; label: string }[] = [
  { name: "800", label: "800 W" },
  { name: "1200", label: "1200 W" },
  { name: "up", label: "Temp +" },
  { name: "down", label: "Temp −" },
];

const REMOTE_KEY = ["geyser", "remote"] as const;

function RemotePanel() {
  const [checking, setChecking] = useState(false);
  const remote = useQuery({
    queryKey: REMOTE_KEY,
    queryFn: getGeyserRemote,
    enabled: checking,
    retry: false,
  });
  const press = useMutation({ mutationFn: pressGeyserButton });
  const status = remote.data;
  const error = remote.error ?? press.error;

  if (!checking) {
    return (
      <Button variant="outline" size="sm" onClick={() => setChecking(true)}>
        Connect to the ESP32
      </Button>
    );
  }
  return (
    <div className="space-y-3 rounded-lg border border-border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-sm font-medium">
          {remote.isLoading
            ? "Connecting…"
            : status
              ? `Connected${status.rssi != null ? ` · Wi-Fi ${status.rssi} dBm` : ""}`
              : "Not reachable"}
        </p>
        <Button variant="ghost" size="sm" onClick={() => remote.refetch()} disabled={remote.isFetching}>
          Refresh
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        The geyser stays on; 800 W and 1200 W each toggle one element (both on is 2000 W, both off draws nothing). The app never presses On/Off.
      </p>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
        {REMOTE_BUTTONS.map((button) => (
          <Button
            key={button.name}
            variant="outline"
            size="sm"
            onClick={() => press.mutate(button.name)}
            disabled={!status || press.isPending}
          >
            {button.label}
          </Button>
        ))}
      </div>
      {error ? <p className="text-sm text-destructive">{String(error)}</p> : null}
    </div>
  );
}

export function GeyserSettings({ config, saving, onSave }: SettingsProps) {
  const saved: GeyserForm = {
    geyser_enabled: config.geyser_enabled,
    geyser_start_hour: config.geyser_start_hour,
    geyser_end_hour: config.geyser_end_hour,
    geyser_low_watts: config.geyser_low_watts,
    geyser_high_watts: config.geyser_high_watts,
    geyser_daily_kwh: config.geyser_daily_kwh,
    geyser_tank_litres: config.geyser_tank_litres,
    geyser_winter_only: config.geyser_winter_only,
    inverter_max_w: config.inverter_max_w,
    geyser_esp32_host: config.geyser_esp32_host,
    geyser_auto_control: config.geyser_auto_control,
    geyser_first_weekdays: config.geyser_first_weekdays,
    geyser_first_below_c: config.geyser_first_below_c,
  };
  const { form, set, dirty, draft, reset } = useDraft(saved);
  const errors = {
    geyser_low_watts: rangeError(GEYSER_RANGES.geyser_low_watts, form.geyser_low_watts),
    geyser_high_watts: rangeError(GEYSER_RANGES.geyser_high_watts, form.geyser_high_watts),
    geyser_daily_kwh: rangeError(GEYSER_RANGES.geyser_daily_kwh, form.geyser_daily_kwh),
    geyser_tank_litres: rangeError(GEYSER_RANGES.geyser_tank_litres, form.geyser_tank_litres),
    inverter_max_w: rangeError(GEYSER_RANGES.inverter_max_w, form.inverter_max_w),
    geyser_first_below_c: rangeError(GEYSER_RANGES.geyser_first_below_c, form.geyser_first_below_c),
  };
  const hoursProblem = form.geyser_end_hour <= form.geyser_start_hour ? "End must be after start" : null;
  const hasErrors = Object.values(errors).some(Boolean) || hoursProblem != null;
  const hasHost = form.geyser_esp32_host.trim() !== "";

  return (
    <SettingsCard
      eyebrow="Geyser"
      title="When to heat water from the sun"
      dirty={dirty}
      hasErrors={hasErrors}
      saving={saving}
      onDiscard={reset}
      onSave={() => {
        if (hasErrors || !draft) return;
        onSave({ ...config, ...draft, geyser_auto_control: draft.geyser_auto_control && draft.geyser_esp32_host.trim() !== "" });
        reset();
      }}
    >
      <SwitchRow
        id="geyser-enabled"
        label="Geyser planning"
        help="Picks the sunniest stretch and the power (800, 1200 or 2000 W) the spare sun can carry, only from sun beyond what fills the battery by sunset, and says when to switch off. Off from sunset."
        checked={form.geyser_enabled}
        onChange={(value) => set("geyser_enabled", value)}
      />
      <SwitchRow
        id="geyser-winter-only"
        label="Winter only"
        help="November to February the geyser heats every day: on the sun when there's enough, with the grid topping up when there isn't. The rest of the year the app leaves it alone; switch it on by hand if you want it."
        checked={form.geyser_winter_only}
        onChange={(value) => set("geyser_winter_only", value)}
      />
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <HourField
          id="geyser-start"
          label="Heat from"
          help="No heating before this."
          value={form.geyser_start_hour}
          onChange={(value) => set("geyser_start_hour", value)}
        />
        <HourField
          id="geyser-end"
          label="Heat until"
          help="Leaves the late sun for the battery."
          value={form.geyser_end_hour}
          onChange={(value) => set("geyser_end_hour", value)}
        />
        <NumberField
          id="geyser-kwh"
          label="Energy to heat it (kWh)"
          help="Roughly what a day's heating takes. The card shows what it really used when the thermostat cut out."
          value={form.geyser_daily_kwh}
          error={errors.geyser_daily_kwh}
          onChange={(value) => set("geyser_daily_kwh", value)}
        />
        <NumberField
          id="geyser-litres"
          label="Tank size (litres)"
          help="Used to work out how long heating takes."
          value={form.geyser_tank_litres}
          error={errors.geyser_tank_litres}
          onChange={(value) => set("geyser_tank_litres", value)}
        />
        <NumberField
          id="geyser-low"
          label="Small element (W)"
          help="One switch on its own."
          value={form.geyser_low_watts}
          error={errors.geyser_low_watts}
          onChange={(value) => set("geyser_low_watts", value)}
        />
        <NumberField
          id="geyser-high"
          label="Large element (W)"
          help="The other switch. Both together is the full power."
          value={form.geyser_high_watts}
          error={errors.geyser_high_watts}
          onChange={(value) => set("geyser_high_watts", value)}
        />
      </div>
      {hoursProblem ? <p className="text-sm text-destructive">{hoursProblem}</p> : null}

      <div className="space-y-3">
        <SectionTitle eyebrow="Before the hotplate" title="Days the geyser heats first" />
        <div className="space-y-1.5">
          <Label>Every week on</Label>
          <ToggleGroup
            aria-label="Days the geyser goes ahead of the hotplate"
            multiple
            value={form.geyser_first_weekdays.map(String)}
            onValueChange={(next) =>
              set(
                "geyser_first_weekdays",
                next.map(Number).sort((a, b) => a - b),
              )
            }
            spacing={0.5}
            className="flex-wrap rounded-md border border-border p-0.5"
          >
            {WEEKDAYS.map((day, index) => (
              <ToggleGroupItem key={day} value={String(index)} size="sm" className="h-auto rounded px-2.5 py-1 font-medium">
                {day}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <p className="text-micro leading-snug text-muted-foreground">Friday: everyone bathes.</p>
        </div>
        <NumberField
          id="geyser-first-cold"
          label="And when the night drops below (°C)"
          help="Last night's low from the forecast. In winter the geyser goes first."
          value={form.geyser_first_below_c}
          error={errors.geyser_first_below_c}
          onChange={(value) => set("geyser_first_below_c", value)}
        />
      </div>

      <div className="space-y-3">
        <SectionTitle eyebrow="ESP32" title="Let the app press the remote's buttons" />
        <TextField
          id="geyser-host"
          label="ESP32 address"
          help="geyser.local, or its IP. Leave empty for reminders only."
          placeholder="192.168.1.100"
          value={form.geyser_esp32_host}
          onChange={(value) => set("geyser_esp32_host", value)}
        />
        <NumberField
          id="inverter-max"
          label="Inverter's most output (W)"
          help="The geyser only gets what's left under 90% of this, and switches off while a big load like the hotplate is on, so the inverter doesn't trip."
          value={form.inverter_max_w}
          error={errors.inverter_max_w}
          onChange={(value) => set("inverter_max_w", value)}
        />
        <SwitchRow
          id="geyser-auto"
          label="Switch the geyser automatically"
          help="Leave the geyser switched on: the app heats at 800, 1200 or 2000 W and stops it by toggling the elements as the plan says, never pressing On/Off, with a notification for every press. Each press is checked against the house load; if the geyser doesn't respond, it goes back to reminders for the day. Not in dry run."
          checked={form.geyser_auto_control && hasHost}
          disabled={!hasHost}
          onChange={(value) => set("geyser_auto_control", value)}
        />
        {config.geyser_esp32_host ? (
          <RemotePanel />
        ) : (
          <p className="text-micro text-muted-foreground">Save an address to test the remote's buttons.</p>
        )}
      </div>
    </SettingsCard>
  );
}

type HotplateForm = Pick<
  AutomationConfig,
  | "hotplate_enabled"
  | "hotplate_watts"
  | "hotplate_hours"
  | "hotplate_start_hour"
  | "hotplate_end_hour"
  | "hotplate_every_days"
  | "day_low_soc"
>;

const HOTPLATE_RANGES = {
  hotplate_watts: { min: 100, max: 5_000 },
  hotplate_hours: { min: 0.5, max: 6 },
  hotplate_every_days: { min: 1, max: 7 },
} satisfies Record<string, Range>;

export function HotplateSettings({ config, saving, onSave }: SettingsProps) {
  const saved: HotplateForm = {
    hotplate_enabled: config.hotplate_enabled,
    hotplate_watts: config.hotplate_watts,
    hotplate_hours: config.hotplate_hours,
    hotplate_start_hour: config.hotplate_start_hour,
    hotplate_end_hour: config.hotplate_end_hour,
    hotplate_every_days: config.hotplate_every_days,
    day_low_soc: config.day_low_soc,
  };
  const { form, set, dirty, draft, reset } = useDraft(saved);
  const errors = {
    hotplate_watts: rangeError(HOTPLATE_RANGES.hotplate_watts, form.hotplate_watts),
    hotplate_hours: rangeError(HOTPLATE_RANGES.hotplate_hours, form.hotplate_hours),
    hotplate_every_days: rangeError(HOTPLATE_RANGES.hotplate_every_days, form.hotplate_every_days),
    day_low_soc: rangeError({ min: config.min_soc_percent, max: 100 }, form.day_low_soc),
  };
  const hoursProblem = form.hotplate_end_hour <= form.hotplate_start_hour ? "End must be after start" : null;
  const hasErrors = Object.values(errors).some(Boolean) || hoursProblem != null;

  return (
    <SettingsCard
      eyebrow="Hotplate"
      title="Cooking on the sun"
      dirty={dirty}
      hasErrors={hasErrors}
      saving={saving}
      onDiscard={reset}
      onSave={() => {
        if (hasErrors || !draft) return;
        onSave({ ...config, ...draft });
        reset();
      }}
    >
      <SwitchRow
        id="hotplate-enabled"
        label="Hotplate planning"
        help="On cooking days the app plans a session at the highest level the sun can carry, or says cook on gas. A load it didn't ask for in the cooking hours makes it ask whether the hotplate is on."
        checked={form.hotplate_enabled}
        onChange={(value) => set("hotplate_enabled", value)}
      />
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <NumberField
          id="hotplate-watts"
          label="Usual level (W)"
          help="What you mostly cook at. It runs 100–2200 W."
          value={form.hotplate_watts}
          error={errors.hotplate_watts}
          onChange={(value) => set("hotplate_watts", value)}
        />
        <NumberField
          id="hotplate-hours"
          label="A session (hours)"
          help="How long cooking usually takes."
          value={form.hotplate_hours}
          error={errors.hotplate_hours}
          onChange={(value) => set("hotplate_hours", value)}
        />
        <NumberField
          id="hotplate-every"
          label="Cook every (days)"
          help="2 = every other day, counted from the last time you said yes."
          value={form.hotplate_every_days}
          error={errors.hotplate_every_days}
          onChange={(value) => set("hotplate_every_days", value)}
        />
        <HourField
          id="hotplate-start"
          label="Cooking from"
          help="The app asks about the hotplate only in these hours."
          value={form.hotplate_start_hour}
          onChange={(value) => set("hotplate_start_hour", value)}
        />
        <HourField
          id="hotplate-end"
          label="Cooking until"
          help="A planned session ends by then."
          value={form.hotplate_end_hour}
          onChange={(value) => set("hotplate_end_hour", value)}
        />
        <NumberField
          id="day-low-soc"
          label="Lowest battery in the day (%)"
          help="For the hotplate, geyser and scooter: the battery may go this low in the day only if the afternoon sun fills it again."
          value={form.day_low_soc}
          error={errors.day_low_soc}
          onChange={(value) => set("day_low_soc", value)}
        />
      </div>
      {hoursProblem ? <p className="text-sm text-destructive">{hoursProblem}</p> : null}
    </SettingsCard>
  );
}

type ScooterForm = Pick<
  AutomationConfig,
  | "ev_enabled"
  | "ev_battery_kwh"
  | "ev_charger_watts"
  | "ev_daily_use_pct"
  | "ev_ble_name"
  | "ev_ble_mac"
  | "ev_power_cycle_reads"
  | "ev_daily_check"
  | "ev_check_minute"
  | "ev_no_charge_above_pct"
  | "ev_remind_minutes"
>;

const SCOOTER_RANGES = {
  ev_battery_kwh: { min: 0.2, max: 100 },
  ev_charger_watts: { min: 100, max: 10_000 },
  ev_daily_use_pct: { min: 1, max: 100 },
  ev_no_charge_above_pct: { min: 0, max: 100 },
  ev_remind_minutes: { min: 5, max: 120 },
} satisfies Record<string, Range>;

function minuteToTime(minute: number): string {
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

function timeToMinute(text: string): number | null {
  const [hours, minutes] = text.split(":").map(Number);
  return Number.isFinite(hours) && Number.isFinite(minutes) ? hours * 60 + minutes : null;
}

function ScooterTest() {
  const queryClient = useQueryClient();
  const test = useMutation({
    mutationFn: () => readScooterNow(true),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["automation", "status"] }),
  });
  const reading = test.data;
  return (
    <div className="space-y-2 rounded-lg border border-border p-3">
      <p className="text-sm">
        Stand next to the scooter. This switches it <strong>on</strong>, reads the charge, then switches it <strong>off</strong>. Check that
        it really goes off before turning on live reads below.
      </p>
      <Button size="sm" variant="outline" onClick={() => test.mutate()} disabled={test.isPending}>
        {test.isPending ? "Testing… (about 20 s)" : "Test on → read → off"}
      </Button>
      {reading ? (
        <div className="space-y-0.5 text-xs tabular-nums text-muted-foreground">
          <p>
            {reading.soc != null ? `${reading.soc.toFixed(1)}%` : "No live charge in the frames"} at {clock(reading.read_at)} · OFF{" "}
            {reading.off_sent ? "sent" : "not sent"}
          </p>
          <p>
            State bytes: before {reading.state_before ?? "–"} · on {reading.state_on ?? "–"} · after {reading.state_after ?? "–"}
          </p>
        </div>
      ) : null}
      {test.error ? <p className="text-sm text-destructive">{String(test.error)}</p> : null}
    </div>
  );
}

export function ScooterSettings({ config, saving, onSave }: SettingsProps) {
  const saved: ScooterForm = {
    ev_enabled: config.ev_enabled,
    ev_battery_kwh: config.ev_battery_kwh,
    ev_charger_watts: config.ev_charger_watts,
    ev_daily_use_pct: config.ev_daily_use_pct,
    ev_ble_name: config.ev_ble_name,
    ev_ble_mac: config.ev_ble_mac,
    ev_power_cycle_reads: config.ev_power_cycle_reads,
    ev_daily_check: config.ev_daily_check,
    ev_check_minute: config.ev_check_minute,
    ev_no_charge_above_pct: config.ev_no_charge_above_pct,
    ev_remind_minutes: config.ev_remind_minutes,
  };
  const { form, set, dirty, draft, reset } = useDraft(saved);
  const errors = {
    ev_battery_kwh: rangeError(SCOOTER_RANGES.ev_battery_kwh, form.ev_battery_kwh),
    ev_charger_watts: rangeError(SCOOTER_RANGES.ev_charger_watts, form.ev_charger_watts),
    ev_daily_use_pct: rangeError(SCOOTER_RANGES.ev_daily_use_pct, form.ev_daily_use_pct),
    ev_no_charge_above_pct: rangeError(SCOOTER_RANGES.ev_no_charge_above_pct, form.ev_no_charge_above_pct),
    ev_remind_minutes: rangeError(SCOOTER_RANGES.ev_remind_minutes, form.ev_remind_minutes),
  };
  const nameProblem = form.ev_ble_name.trim() === "" ? "Enter the scooter's Bluetooth name" : null;
  const timeProblem = form.ev_check_minute < 360 || form.ev_check_minute > 960 ? "Pick a time between 6 AM and 4 PM" : null;
  const hasErrors = Object.values(errors).some(Boolean) || nameProblem != null || (form.ev_daily_check && timeProblem != null);

  return (
    <SettingsCard
      eyebrow="Scooter"
      title="Charging the scooter from the sun"
      dirty={dirty}
      hasErrors={hasErrors}
      saving={saving}
      onDiscard={reset}
      onSave={() => {
        if (hasErrors || !draft) return;
        onSave({ ...config, ...draft });
        reset();
      }}
    >
      <SwitchRow
        id="ev-enabled"
        label="Scooter planning"
        help="Solar only. At or below the threshold it charges today; above it, only when tomorrow's sun can't charge it but today's can. While a charge is needed it reminds you until it sees the charger on."
        checked={form.ev_enabled}
        onChange={(value) => set("ev_enabled", value)}
      />
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <NumberField
          id="ev-kwh"
          label="Battery (kWh)"
          help="Yadea T5L: 72 V 30 Ah ≈ 2.16 kWh."
          value={form.ev_battery_kwh}
          error={errors.ev_battery_kwh}
          onChange={(value) => set("ev_battery_kwh", value)}
        />
        <NumberField
          id="ev-charger"
          label="Charger (W)"
          help="What the charger draws from the house."
          value={form.ev_charger_watts}
          error={errors.ev_charger_watts}
          onChange={(value) => set("ev_charger_watts", value)}
        />
        <NumberField
          id="ev-daily"
          label="A day's riding (%)"
          help="How much of the battery a normal day uses."
          value={form.ev_daily_use_pct}
          error={errors.ev_daily_use_pct}
          onChange={(value) => set("ev_daily_use_pct", value)}
        />
        <TextField
          id="ev-name"
          label="Bluetooth name"
          help={nameProblem ?? "Shown by the scooter's Bluetooth module."}
          value={form.ev_ble_name}
          onChange={(value) => set("ev_ble_name", value)}
        />
        <TextField
          id="ev-mac"
          label="MAC address (optional)"
          help="Only if reading fails: normally it comes from the module's serial number."
          placeholder="C1:3A:64:07:15:40"
          value={form.ev_ble_mac}
          onChange={(value) => set("ev_ble_mac", value)}
        />
      </div>
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <NumberField
          id="ev-threshold"
          label="No charge above (%)"
          help="Above this the scooter only charges when tomorrow looks cloudy and today is sunny."
          value={form.ev_no_charge_above_pct}
          error={errors.ev_no_charge_above_pct}
          onChange={(value) => set("ev_no_charge_above_pct", value)}
        />
        <NumberField
          id="ev-remind"
          label="Remind every (min)"
          help={'How often "plug in the scooter" repeats until it\'s charging.'}
          value={form.ev_remind_minutes}
          error={errors.ev_remind_minutes}
          onChange={(value) => set("ev_remind_minutes", value)}
        />
        <div className="space-y-1.5">
          <Label htmlFor="ev-check-time">Daily check at</Label>
          <Input
            id="ev-check-time"
            type="time"
            disabled={!form.ev_daily_check}
            value={minuteToTime(form.ev_check_minute)}
            onChange={(event) => {
              const minute = timeToMinute(event.target.value);
              if (minute != null) set("ev_check_minute", minute);
            }}
          />
          <p className="text-micro leading-snug text-muted-foreground">
            {timeProblem ?? "Retried twice, 30 min apart, if the scooter isn't near."}
          </p>
        </div>
      </div>
      <SwitchRow
        id="ev-daily"
        label="Read the scooter once a day"
        help="Switches it on for a few seconds (it beeps), reads its %, and switches it off, once a day at the time above."
        checked={form.ev_daily_check}
        onChange={(value) => set("ev_daily_check", value)}
      />
      <SwitchRow
        id="ev-live"
        label="Read a live % while charging"
        help="The scooter only updates its % while switched on, so this switches it on, reads, and switches it off again about once an hour while the charger runs. Without it the app reads the stored % and estimates the charging."
        checked={form.ev_power_cycle_reads}
        onChange={(value) => set("ev_power_cycle_reads", value)}
      />
      <ScooterTest />
    </SettingsCard>
  );
}

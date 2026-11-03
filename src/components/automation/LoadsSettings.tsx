import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input, Label } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { getGeyserRemote, learnGeyserButton, pressGeyserButton, readScooterNow } from "@/lib/tauri";
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
              <p className="text-sm text-muted-foreground">{hasErrors ? "Fix the highlighted fields to save." : "You have unsaved changes."}</p>
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

function SwitchRow({ id, label, help, checked, disabled, onChange }: { id: string; label: string; help: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
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

function TextField({ id, label, help, value, placeholder, onChange }: { id: string; label: string; help: string; value: string; placeholder?: string; onChange: (value: string) => void }) {
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
  | "geyser_esp32_host"
  | "geyser_auto_control"
>;

const GEYSER_RANGES = {
  geyser_low_watts: { min: 100, max: 5_000 },
  geyser_high_watts: { min: 100, max: 5_000 },
  geyser_daily_kwh: { min: 0.5, max: 20 },
} satisfies Record<string, Range>;

const REMOTE_BUTTONS: { name: GeyserButton; label: string }[] = [
  { name: "power", label: "On/Off" },
  { name: "w800", label: "800 W" },
  { name: "w1200", label: "1200 W" },
  { name: "w2000", label: "2000 W" },
  { name: "temp_up", label: "Temp +" },
  { name: "temp_down", label: "Temp −" },
];

const REMOTE_KEY = ["geyser", "remote"] as const;

function RemotePanel() {
  const queryClient = useQueryClient();
  const [checking, setChecking] = useState(false);
  const remote = useQuery({
    queryKey: REMOTE_KEY,
    queryFn: getGeyserRemote,
    enabled: checking,
    retry: false,
    refetchInterval: (query) => (query.state.data?.learning ? 2_000 : false),
  });
  const learn = useMutation({
    mutationFn: learnGeyserButton,
    onSuccess: (status) => queryClient.setQueryData(REMOTE_KEY, status),
  });
  const press = useMutation({ mutationFn: pressGeyserButton });
  const status = remote.data;
  const error = remote.error ?? learn.error ?? press.error;

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
          {remote.isLoading ? "Connecting…" : status ? `Connected${status.rssi != null ? ` · Wi-Fi ${status.rssi} dBm` : ""}` : "Not reachable"}
        </p>
        <Button variant="ghost" size="sm" onClick={() => remote.refetch()} disabled={remote.isFetching}>
          Refresh
        </Button>
      </div>
      {status?.learning ? (
        <p className="text-sm">
          Point the geyser's remote at the ESP32 and press <strong>{REMOTE_BUTTONS.find((b) => b.name === status.learning)?.label}</strong> now (20 seconds).
        </p>
      ) : status?.last_learn ? (
        <p className="text-xs text-muted-foreground">
          {status.last_learn.ok ? `Learned ${status.last_learn.button}.` : `Learning ${status.last_learn.button} failed: ${status.last_learn.error ?? "no signal"}.`}
        </p>
      ) : null}
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {REMOTE_BUTTONS.map((button) => {
          const learned = status?.learned[button.name] ?? false;
          return (
            <div key={button.name} className="flex items-center justify-between gap-2 rounded-md border border-border px-2.5 py-2">
              <span className="inline-flex items-center gap-2 text-sm">
                {button.label}
                <Badge variant={learned ? "success" : "secondary"}>{learned ? "Learned" : "Not learned"}</Badge>
              </span>
              <span className="flex gap-1">
                <Button variant="ghost" size="sm" onClick={() => learn.mutate(button.name)} disabled={!status || !!status.learning || learn.isPending}>
                  Learn
                </Button>
                <Button variant="outline" size="sm" onClick={() => press.mutate(button.name)} disabled={!learned || press.isPending}>
                  Press
                </Button>
              </span>
            </div>
          );
        })}
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
    geyser_esp32_host: config.geyser_esp32_host,
    geyser_auto_control: config.geyser_auto_control,
  };
  const { form, set, dirty, draft, reset } = useDraft(saved);
  const errors = {
    geyser_low_watts: rangeError(GEYSER_RANGES.geyser_low_watts, form.geyser_low_watts),
    geyser_high_watts: rangeError(GEYSER_RANGES.geyser_high_watts, form.geyser_high_watts),
    geyser_daily_kwh: rangeError(GEYSER_RANGES.geyser_daily_kwh, form.geyser_daily_kwh),
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
        help="Picks the sunniest stretch and the power (800, 1200 or 2000 W) the spare sun can carry, keeps what the battery needs for tonight, and says when to switch off. Off from sunset."
        checked={form.geyser_enabled}
        onChange={(value) => set("geyser_enabled", value)}
      />
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <HourField id="geyser-start" label="Heat from" help="No heating before this." value={form.geyser_start_hour} onChange={(value) => set("geyser_start_hour", value)} />
        <HourField id="geyser-end" label="Heat until" help="Leaves the late sun for the battery." value={form.geyser_end_hour} onChange={(value) => set("geyser_end_hour", value)} />
        <NumberField
          id="geyser-kwh"
          label="Energy to heat it (kWh)"
          help="Roughly what a day's heating takes. The card shows what it really used when the thermostat cut out."
          value={form.geyser_daily_kwh}
          error={errors.geyser_daily_kwh}
          onChange={(value) => set("geyser_daily_kwh", value)}
        />
        <NumberField id="geyser-low" label="Small element (W)" help="One switch on its own." value={form.geyser_low_watts} error={errors.geyser_low_watts} onChange={(value) => set("geyser_low_watts", value)} />
        <NumberField id="geyser-high" label="Large element (W)" help="The other switch. Both together is the full power." value={form.geyser_high_watts} error={errors.geyser_high_watts} onChange={(value) => set("geyser_high_watts", value)} />
      </div>
      {hoursProblem ? <p className="text-sm text-destructive">{hoursProblem}</p> : null}

      <div className="space-y-3">
        <SectionTitle eyebrow="ESP32" title="Let the app press the remote's buttons" />
        <TextField
          id="geyser-host"
          label="ESP32 address"
          help="solarhub-geyser.local, or its IP. Leave empty for reminders only."
          placeholder="solarhub-geyser.local"
          value={form.geyser_esp32_host}
          onChange={(value) => set("geyser_esp32_host", value)}
        />
        <SwitchRow
          id="geyser-auto"
          label="Switch the geyser automatically"
          help="Needs On/Off, 800, 1200 and 2000 W learned. Every press is checked against the house load; if the geyser doesn't respond, it goes back to reminders for the day. Not in dry run."
          checked={form.geyser_auto_control && hasHost}
          disabled={!hasHost}
          onChange={(value) => set("geyser_auto_control", value)}
        />
        {config.geyser_esp32_host ? <RemotePanel /> : <p className="text-micro text-muted-foreground">Save an address to learn the remote's buttons.</p>}
      </div>
    </SettingsCard>
  );
}

type ScooterForm = Pick<
  AutomationConfig,
  "ev_enabled" | "ev_battery_kwh" | "ev_charger_watts" | "ev_daily_use_pct" | "ev_ble_name" | "ev_ble_mac" | "ev_power_cycle_reads"
>;

const SCOOTER_RANGES = {
  ev_battery_kwh: { min: 0.2, max: 100 },
  ev_charger_watts: { min: 100, max: 10_000 },
  ev_daily_use_pct: { min: 1, max: 100 },
} satisfies Record<string, Range>;

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
        Stand next to the scooter. This switches it <strong>on</strong>, reads the charge, then switches it <strong>off</strong>. Check that it
        really goes off before turning on live reads below.
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
  };
  const { form, set, dirty, draft, reset } = useDraft(saved);
  const errors = {
    ev_battery_kwh: rangeError(SCOOTER_RANGES.ev_battery_kwh, form.ev_battery_kwh),
    ev_charger_watts: rangeError(SCOOTER_RANGES.ev_charger_watts, form.ev_charger_watts),
    ev_daily_use_pct: rangeError(SCOOTER_RANGES.ev_daily_use_pct, form.ev_daily_use_pct),
  };
  const nameProblem = form.ev_ble_name.trim() === "" ? "Enter the scooter's Bluetooth name" : null;
  const hasErrors = Object.values(errors).some(Boolean) || nameProblem != null;

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
        help="Solar only. The AI weighs the scooter's charge and your daily riding against today's spare sun and the next three days, and can say charge now even at 80% if the coming days look cloudy."
        checked={form.ev_enabled}
        onChange={(value) => set("ev_enabled", value)}
      />
      <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
        <NumberField id="ev-kwh" label="Battery (kWh)" help="Yadea T5L: 72 V 30 Ah ≈ 2.16 kWh." value={form.ev_battery_kwh} error={errors.ev_battery_kwh} onChange={(value) => set("ev_battery_kwh", value)} />
        <NumberField id="ev-charger" label="Charger (W)" help="What the charger draws from the house." value={form.ev_charger_watts} error={errors.ev_charger_watts} onChange={(value) => set("ev_charger_watts", value)} />
        <NumberField id="ev-daily" label="A day's riding (%)" help="How much of the battery a normal day uses." value={form.ev_daily_use_pct} error={errors.ev_daily_use_pct} onChange={(value) => set("ev_daily_use_pct", value)} />
        <TextField id="ev-name" label="Bluetooth name" help={nameProblem ?? "Shown by the scooter's Bluetooth module."} value={form.ev_ble_name} onChange={(value) => set("ev_ble_name", value)} />
        <TextField
          id="ev-mac"
          label="MAC address (optional)"
          help="Only if reading fails: normally it comes from the module's serial number."
          placeholder="C1:3A:64:07:15:40"
          value={form.ev_ble_mac}
          onChange={(value) => set("ev_ble_mac", value)}
        />
      </div>
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

import { Fragment, useState, type ReactNode } from "react";
import { GeyserHeatTime } from "./GeyserHeatTime";
import { clock, modelLabel } from "@/components/automation/shared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger } from "@/components/ui/select";
import type {
  AutomationConfig,
  EditState,
  ElementState,
  GeyserCheck,
  GeyserElements,
  LoadEdit,
  LoadKind,
  LoadPhase,
  LoadsAiStatus,
  LoadsStatus,
  SocBasis,
  WaterPhase,
  WaterStatus,
} from "@/lib/types";
import { Bike, Check, CookingPot, Droplets, Ellipsis, Flame, PlugZap, RefreshCw, Sparkles, Sun, type LucideIcon } from "lucide-react";
import { spareNowText, type LoadLine } from "./spareSun";

type BadgeVariant = "secondary" | "info" | "success" | "warning";

interface BadgeSpec {
  label: string;
  variant: BadgeVariant;
}

const WATER_PHASES: Record<WaterPhase, BadgeSpec> = {
  off: { label: "Off", variant: "secondary" },
  waiting: { label: "Waiting", variant: "secondary" },
  ready: { label: "Turn on", variant: "info" },
  done: { label: "Done", variant: "success" },
  missed: { label: "Not filled", variant: "warning" },
};

const LOAD_PHASES: Record<LoadPhase, BadgeSpec> = {
  off: { label: "Off", variant: "secondary" },
  waiting: { label: "Waiting", variant: "secondary" },
  ready: { label: "Turn on", variant: "info" },
  running: { label: "On", variant: "success" },
  stop: { label: "Turn off", variant: "warning" },
  done: { label: "Done", variant: "success" },
  skipped: { label: "Not today", variant: "secondary" },
  ask: { label: "Cooking?", variant: "info" },
};

const SOC_BASIS: Record<SocBasis, string> = {
  read: "read",
  live: "read live",
  estimated: "estimated",
  full: "charger finished",
  set: "set by you",
};

const HOTPLATE_LEVELS = [200, 400, 800, 1000, 1200, 1400, 1600, 1800, 2000];

interface MenuOption {
  value: string;
  label: string;
}

function stateOptions(kind: LoadKind, geyserLevels: number[]): MenuOption[] {
  switch (kind) {
    case "water":
      return [
        { value: "on", label: "Motor on" },
        { value: "off", label: "Not filled yet" },
        { value: "done", label: "Filled today" },
      ];
    case "geyser":
      return [
        ...geyserLevels.map((watts) => ({ value: `on:${watts}`, label: `On at ${watts} W` })),
        { value: "off", label: "Off, not hot yet" },
        { value: "done", label: "Hot enough" },
      ];
    case "hotplate":
      return [
        ...HOTPLATE_LEVELS.map((watts) => ({ value: `on:${watts}`, label: `On at ${watts} W` })),
        { value: "done", label: "Cooked today" },
        { value: "off", label: "Not cooked today" },
      ];
    case "ev":
      return [
        { value: "on", label: "Charging" },
        { value: "off", label: "Not charging" },
      ];
  }
}

function nearest(levels: number[], watts: number): number {
  return levels.reduce((best, level) => (Math.abs(level - watts) < Math.abs(best - watts) ? level : best));
}

/** What the menu shows as true now, from what the app believes. */
function currentValue(kind: LoadKind, loads: LoadsStatus, water: WaterStatus, geyserLevels: number[]): string {
  switch (kind) {
    case "water": {
      if (loads.water_running_since != null) return "on";
      const done = loads.ai.mode === "ai" ? loads.water.phase === "done" : water.phase === "done";
      return done ? "done" : "off";
    }
    case "geyser":
      if (loads.geyser.running_w != null) return `on:${nearest(geyserLevels, loads.geyser.running_w)}`;
      return loads.geyser.phase === "done" ? "done" : "off";
    case "hotplate":
      if (loads.hotplate.running_w != null) return `on:${nearest(HOTPLATE_LEVELS, loads.hotplate.running_w)}`;
      return loads.hotplate.phase === "done" ? "done" : "off";
    case "ev":
      return loads.ev.running_w != null ? "on" : "off";
  }
}

function toEdit(kind: LoadKind, value: string): LoadEdit {
  const [state, watts] = value.split(":");
  return { load: kind, state: state as EditState, watts: watts ? Number(watts) : null, kwh: null, soc: null };
}

function waterHeadline(water: WaterStatus): string {
  switch (water.phase) {
    case "off":
      return "Reminders are off";
    case "done":
      return "Tank filled today";
    case "missed":
      return "Tank not filled today";
    case "ready":
      return "Good solar: turn on the motor";
    default:
      return "Waiting for good solar";
  }
}

function minutesSince(local: string): number {
  return Math.max(0, (Date.now() - new Date(local).getTime()) / 60_000);
}

function readToday(local: string | null): boolean {
  return local != null && new Date(local).toDateString() === new Date().toDateString();
}

function durationText(minutes: number): string {
  const rounded = Math.round(minutes);
  const hours = Math.floor(rounded / 60);
  if (hours === 0) return `${rounded} min`;
  return rounded % 60 === 0 ? `${hours} h` : `${hours} h ${rounded % 60} min`;
}

interface Meter {
  name: string;
  value: number;
  max: number;
  marker?: number;
  label: string;
}

interface Extra {
  value: string;
  label: string;
  run: () => void;
}

interface Primary {
  label: string;
  icon: LucideIcon;
  run: () => void;
  busy?: boolean;
}

interface LoadItem {
  kind: LoadKind;
  icon: LucideIcon;
  name: string;
  badge: BadgeSpec;
  line: string;
  status?: string;
  meter?: Meter;
  lights?: { elements: GeyserElements; levels: [number, number]; onToggle?: (name: ElementButton) => void; busy?: boolean };
  primary?: Primary;
  extras: Extra[];
}

/** A small bar toward a load's goal, with an optional marker (the scooter's 80%). */
function MeterBar({ meter }: { meter: Meter }) {
  const share = (amount: number) => `${Math.min(100, Math.max(0, (amount / meter.max) * 100))}%`;
  return (
    <div className="flex items-center gap-2">
      <div
        className="relative h-1.5 w-24 shrink-0 overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-label={meter.name}
        aria-valuemin={0}
        aria-valuemax={meter.max}
        aria-valuenow={Math.round(meter.value * 10) / 10}
      >
        <div className="h-full rounded-full bg-primary" style={{ width: share(meter.value) }} />
        {meter.marker != null ? (
          <span className="absolute inset-y-0 w-0.5 bg-foreground/50" style={{ left: share(meter.marker) }} aria-hidden />
        ) : null}
      </div>
      <span className="truncate text-xs text-muted-foreground tabular-nums">{meter.label}</span>
    </div>
  );
}

const ELEMENT_STATES: Record<ElementState, string> = {
  heating: "heating",
  keep_warm: "on, thermostat cut out (blinking)",
  off: "off",
};

type ElementButton = "800" | "1200";

function ElementDot({ state }: { state: ElementState }) {
  return (
    <span
      className={
        state === "off"
          ? "size-2.5 rounded-full border border-border bg-muted"
          : state === "heating"
            ? "size-2.5 rounded-full bg-primary"
            : "size-2.5 animate-pulse rounded-full bg-primary motion-reduce:animate-none"
      }
      aria-hidden
    />
  );
}

/** The two element switches as the app believes they are, like the panel's own lights; with the ESP32, each one presses its button. */
function ElementLights({
  elements,
  levels,
  onToggle,
  busy,
}: {
  elements: GeyserElements;
  levels: [number, number];
  onToggle?: (name: ElementButton) => void;
  busy?: boolean;
}) {
  const lights: [number, ElementState, ElementButton][] = [
    [levels[0], elements.low, "800"],
    [levels[1], elements.high, "1200"],
  ];
  const when = elements.at ? ` at ${clock(elements.at)}` : "";
  const summary = lights.map(([watts, state]) => `${watts} W ${ELEMENT_STATES[state]}`).join(", ");
  return (
    <div
      className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
      role={onToggle ? "group" : "img"}
      aria-label={`Elements: ${summary}${elements.unsure ? ", not sure" : ""}. From ${elements.why}${when}.`}
    >
      {lights.map(([watts, state, name]) =>
        onToggle ? (
          <Toggle
            key={watts}
            size="sm"
            variant="outline"
            className="tabular-nums"
            pressed={state !== "off"}
            onPressedChange={() => onToggle(name)}
            disabled={busy}
            aria-label={`${watts} W element, ${ELEMENT_STATES[state]}. Press to switch it ${state === "off" ? "on" : "off"}.`}
          >
            <ElementDot state={state} />
            {watts}
          </Toggle>
        ) : (
          <span key={watts} className="inline-flex items-center gap-1.5 tabular-nums">
            <ElementDot state={state} />
            {watts}
          </span>
        ),
      )}
      <span className="truncate">
        {elements.unsure ? "Not sure: " : ""}
        {elements.why}
        {elements.unsure ? "" : when}
      </span>
    </div>
  );
}

/** The ⋯ menu: set what's really true, plus the rarer actions. */
function MoreMenu({
  item,
  value,
  options,
  pending,
  onEdit,
}: {
  item: LoadItem;
  value: string;
  options: MenuOption[];
  pending: boolean;
  onEdit: (edit: LoadEdit) => void;
}) {
  return (
    <Select
      value={value}
      onValueChange={(next) => {
        if (typeof next !== "string" || next === value) return;
        const extra = item.extras.find((candidate) => candidate.value === next);
        if (extra) extra.run();
        else onEdit(toEdit(item.kind, next));
      }}
    >
      <SelectTrigger size="sm" disabled={pending} aria-label={`More for the ${item.name.toLowerCase()}`}>
        <Ellipsis />
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false} align="end">
        <SelectGroup>
          <SelectLabel>What's true now</SelectLabel>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectGroup>
        {item.extras.length > 0 ? (
          <>
            <SelectSeparator />
            <SelectGroup>
              {item.extras.map((extra) => (
                <SelectItem key={extra.value} value={extra.value}>
                  {extra.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </>
        ) : null}
      </SelectContent>
    </Select>
  );
}

const CHECK_STATES: Record<GeyserCheck["state"], string> = {
  checking: "checking the load…",
  confirmed: "confirmed",
  not_confirmed: "not confirmed",
};

function checkText(check: GeyserCheck) {
  const when = check.checked_at && check.state !== "checking" ? ` ${clock(check.checked_at)}` : "";
  return `ESP32: ${check.action} at ${clock(check.sent_at)} · ${CHECK_STATES[check.state]}${when}. ${check.note}`;
}

function LoadRowView({ item, menu }: { item: LoadItem; menu: ReactNode }) {
  const Icon = item.icon;
  const Action = item.primary?.icon;
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-1.5 py-2.5">
      <div className="flex w-full items-center gap-2 sm:w-40 sm:shrink-0">
        <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        <span className="text-sm font-semibold">{item.name}</span>
        <Badge variant={item.badge.variant} className="ml-auto sm:hidden">
          {item.badge.label}
        </Badge>
      </div>
      <Badge variant={item.badge.variant} className="hidden w-20 justify-center sm:inline-flex">
        {item.badge.label}
      </Badge>
      <div className="min-w-0 flex-1 space-y-1">
        <p className="truncate text-sm" title={item.line}>
          {item.line}
        </p>
        {item.status ? <p className="text-xs text-muted-foreground">{item.status}</p> : null}
        {item.meter ? <MeterBar meter={item.meter} /> : null}
        {item.lights ? <ElementLights {...item.lights} /> : null}
      </div>
      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {item.primary && Action ? (
          <Button size="sm" variant="outline" onClick={item.primary.run} disabled={item.primary.busy}>
            <Action className={item.primary.busy ? "animate-spin motion-reduce:animate-none" : undefined} /> {item.primary.label}
          </Button>
        ) : null}
        {menu}
      </div>
    </li>
  );
}

function AiLine({ ai }: { ai: LoadsAiStatus }) {
  const summary =
    ai.mode === "ai"
      ? `AI plan${ai.model ? ` · ${modelLabel(ai.model)}` : ""}${ai.at ? ` · ${clock(ai.at)}` : ""}`
      : ai.mode === "fallback"
        ? "App's own plan · the AI's answer failed the checks"
        : "App's own plan";
  const detail =
    ai.mode === "ai" ? ai.reason : ai.mode === "fallback" ? null : "The AI plans the loads when a Groq or Gemini key is set.";
  const problems = ai.mode === "fallback" ? ai.problems : [];
  return (
    <details className="group text-xs text-muted-foreground">
      <summary className="inline-flex cursor-pointer list-none items-center gap-1.5 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <Sparkles className="size-3" aria-hidden />
        {summary}
        {detail || problems.length ? <span className="underline underline-offset-2 group-open:hidden">Why?</span> : null}
      </summary>
      {detail ? <p className="mt-2 max-w-prose text-sm leading-relaxed">{detail}</p> : null}
      {problems.length ? (
        <div className="mt-2 max-w-prose space-y-1 text-sm leading-relaxed">
          <p>The AI's plan was turned down because:</p>
          <ul className="list-disc space-y-1 pl-5">
            {problems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </details>
  );
}

/** A line on the spare-sun chart for each load still to run today. */
function loadLines(loads: LoadsStatus, water: WaterStatus, config: AutomationConfig | undefined, geyserLevels: number[]): LoadLine[] {
  if (!config) return [];
  const lines: LoadLine[] = [];
  const waterPhase = loads.ai.mode === "ai" ? loads.water.phase : water.phase;
  if (config.water_reminder_enabled && loads.water_running_since == null && waterPhase !== "done" && waterPhase !== "off") {
    lines.push({ watts: config.water_motor_watts, label: "motor" });
  }
  const geyser = loads.geyser;
  if (config.geyser_enabled && geyser.running_w == null && geyser.phase !== "done" && geyser.phase !== "off") {
    lines.push(...geyserLevels.map((watts) => ({ watts, label: "geyser" })));
  }
  const hotplate = loads.hotplate;
  if (
    config.hotplate_enabled &&
    hotplate.cooking_day &&
    hotplate.running_w == null &&
    !["done", "skipped", "off"].includes(hotplate.phase)
  ) {
    lines.push({ watts: config.hotplate_watts, label: "hotplate" });
  }
  if (config.ev_enabled && loads.ev.charge_today === true && loads.ev.running_w == null) {
    lines.push({ watts: config.ev_charger_watts, label: "scooter" });
  }
  return lines;
}

interface SolarLoadsCardProps {
  water: WaterStatus | undefined;
  loads: LoadsStatus | undefined;
  waterPending: boolean;
  geyserPending: boolean;
  scooterPending: boolean;
  skipPending: boolean;
  hotplatePending: boolean;
  editPending: boolean;
  config: AutomationConfig | undefined;
  geyserLevels: number[];
  onWaterDone: () => void;
  onGeyserDone: () => void;
  onSwitchGeyser: (watts: number | null) => void;
  onToggleElement: (name: ElementButton) => void;
  togglePending: boolean;
  switchError: string | null;
  onReadScooter: () => void;
  onSkipScooter: () => void;
  onEdit: (edit: LoadEdit) => void;
  onHotplateDone: () => void;
  onHotplateSkip: () => void;
}

export function SolarLoadsCard({
  water,
  loads,
  waterPending,
  geyserPending,
  scooterPending,
  skipPending,
  hotplatePending,
  editPending,
  config,
  geyserLevels,
  onWaterDone,
  onGeyserDone,
  onSwitchGeyser,
  onToggleElement,
  togglePending,
  switchError,
  onReadScooter,
  onSkipScooter,
  onEdit,
  onHotplateDone,
  onHotplateSkip,
}: SolarLoadsCardProps) {
  const [heatTimeOpen, setHeatTimeOpen] = useState(false);
  if (!water || !loads) {
    return <section className="min-h-32 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }
  const lines = loadLines(loads, water, config, geyserLevels);
  const header = [
    spareNowText(loads.sun_hours, lines),
    loads.full_by
      ? `battery full by ${clock(loads.full_by)}`
      : loads.projected_sunset_soc != null
        ? `battery ${loads.projected_sunset_soc.toFixed(0)}% by sunset`
        : null,
    loads.grid_kwh != null ? `grid tops up about ${loads.grid_kwh.toFixed(1)} kWh` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const planned = loads.ai.mode === "ai" ? loads.water : null;
  const waterDone = planned ? planned.phase === "done" : water.phase === "done";
  const runMinutes = config?.water_run_minutes ?? 30;
  const waterItem: LoadItem = {
    kind: "water",
    icon: Droplets,
    name: "Water tank",
    badge: planned ? LOAD_PHASES[planned.phase] : loads.water_running_since ? LOAD_PHASES.running : WATER_PHASES[water.phase],
    line: planned?.note ?? (loads.water_running_since ? `Motor on since ${clock(loads.water_running_since)}` : waterHeadline(water)),
    meter: loads.water_running_since
      ? {
          name: "Motor run",
          value: minutesSince(loads.water_running_since),
          max: runMinutes,
          label: `${durationText(minutesSince(loads.water_running_since))} of ${runMinutes} min`,
        }
      : undefined,
    primary: !waterDone && water.phase !== "off" ? { label: "Filled", icon: Check, run: onWaterDone, busy: waterPending } : undefined,
    extras: [],
  };

  const geyser = loads.geyser;
  const canSwitch = Boolean(config?.geyser_esp32_host) && !config?.dry_run;
  const geyserItem: LoadItem = {
    kind: "geyser",
    icon: Flame,
    name: geyser.auto ? "Geyser · ESP32" : "Geyser",
    badge: LOAD_PHASES[geyser.phase],
    line: geyser.note ?? "–",
    status: switchError ?? (geyser.check ? checkText(geyser.check) : undefined),
    meter:
      geyser.phase === "off"
        ? undefined
        : {
            name: "Geyser heating today",
            value: geyser.delivered_kwh,
            max: geyser.need_kwh,
            label: `${geyser.delivered_kwh.toFixed(1)} / ${geyser.need_kwh.toFixed(1)} kWh${loads.geyser_first ? " · before the hotplate today" : ""}`,
          },
    lights:
      config && (geyser.elements || canSwitch)
        ? {
            elements: geyser.elements ?? { low: "off", high: "off", unsure: true, why: "the app doesn't know yet", at: null },
            levels: [config.geyser_low_watts, config.geyser_high_watts],
            onToggle: canSwitch ? onToggleElement : undefined,
            busy: togglePending,
          }
        : undefined,
    primary:
      geyser.phase !== "off" && geyser.phase !== "done"
        ? { label: "Hot enough", icon: Check, run: onGeyserDone, busy: geyserPending }
        : undefined,
    extras: [
      ...(canSwitch
        ? [
            ...geyserLevels.map((watts) => ({ value: `x:on-${watts}`, label: `Switch on at ${watts} W`, run: () => onSwitchGeyser(watts) })),
            { value: "x:off", label: "Switch off", run: () => onSwitchGeyser(null) },
          ]
        : []),
      { value: "x:heat-time", label: "Heating time…", run: () => setHeatTimeOpen(true) },
    ],
  };

  const hotplate = loads.hotplate;
  const sessionMinutes = (config?.hotplate_hours ?? 2.5) * 60;
  const cooking = hotplate.phase === "running" || hotplate.phase === "stop";
  const hotplateItem: LoadItem = {
    kind: "hotplate",
    icon: CookingPot,
    name: "Hotplate",
    badge: LOAD_PHASES[hotplate.phase],
    line: hotplate.note ?? "–",
    meter:
      hotplate.running_since && hotplate.running_w != null
        ? {
            name: "Cooking session",
            value: minutesSince(hotplate.running_since),
            max: sessionMinutes,
            label: `${durationText(minutesSince(hotplate.running_since))} · about ${hotplate.running_w.toFixed(0)} W`,
          }
        : undefined,
    primary: cooking ? { label: "Done cooking", icon: Check, run: onHotplateDone, busy: hotplatePending } : undefined,
    extras:
      hotplate.cooking_day && (hotplate.phase === "ready" || hotplate.phase === "waiting")
        ? [{ value: "x:skip", label: "Not cooking today", run: onHotplateSkip }]
        : [],
  };

  const ev = loads.ev;
  const reading = scooterPending || ev.reading;
  const readNow: Primary = { label: reading ? "Reading…" : "Read now", icon: RefreshCw, run: onReadScooter, busy: reading };
  const canSkip = ev.phase === "ready" || ev.phase === "waiting" || ev.phase === "stop";
  const scooterItem: LoadItem = {
    kind: "ev",
    icon: Bike,
    name: "Scooter",
    badge: LOAD_PHASES[ev.phase],
    line: ev.check_failed && !readToday(ev.soc_at)
      ? "Couldn't reach it for today's check. Tap Read now near it."
      : ev.soc == null && ev.read_error
        ? "Can't reach the scooter. Tap Read now near it."
        : (ev.note ?? "–"),
    meter:
      ev.soc != null
        ? {
            name: "Scooter battery",
            value: ev.soc,
            max: 100,
            marker: config?.ev_no_charge_above_pct ?? 80,
            label: `${ev.soc.toFixed(0)}%${ev.soc_basis ? ` · ${SOC_BASIS[ev.soc_basis]}` : ""}${ev.soc_at ? ` ${clock(ev.soc_at)}` : ""}`,
          }
        : undefined,
    primary:
      ev.phase === "off"
        ? undefined
        : ev.phase === "ready"
          ? { label: "Plugged in", icon: PlugZap, run: () => onEdit({ load: "ev", state: "on", watts: null, kwh: null, soc: null }) }
          : readNow,
    extras: [
      ...(ev.phase === "ready" && !reading ? [{ value: "x:read", label: "Read its % now", run: onReadScooter }] : []),
      ...(canSkip && !skipPending ? [{ value: "x:skip", label: "Not today", run: onSkipScooter }] : []),
    ],
  };

  const items = [waterItem, geyserItem, hotplateItem, scooterItem];

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:p-5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3 className="inline-flex items-center gap-2 text-label font-semibold uppercase tracking-wide text-muted-foreground">
          <Sun className="size-4" aria-hidden /> Solar loads
        </h3>
        {header ? <span className="text-xs tabular-nums text-muted-foreground">{header}</span> : null}
      </div>
      <ul className="divide-y divide-border border-y border-border">
        {items.map((item) => (
          <Fragment key={item.kind}>
            <LoadRowView
              item={item}
              menu={
                <MoreMenu
                  item={item}
                  value={currentValue(item.kind, loads, water, geyserLevels)}
                  options={stateOptions(item.kind, geyserLevels)}
                  pending={editPending}
                  onEdit={onEdit}
                />
              }
            />
            {item.kind === "geyser" && heatTimeOpen ? (
              <li className="pb-2.5">
                <GeyserHeatTime
                  litres={config?.geyser_tank_litres ?? 50}
                  levels={geyserLevels}
                  runningW={geyser.running_w}
                  onClose={() => setHeatTimeOpen(false)}
                />
              </li>
            ) : null}
          </Fragment>
        ))}
      </ul>
      <AiLine ai={loads.ai} />
    </section>
  );
}

import { ArrowLeftRight, CircleCheck, TriangleAlert } from "lucide-react";
import { Bar, BarChart, CartesianGrid, Cell, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { CHART_FONT, ChartTooltipBox, VIZ } from "@/components/automation/shared";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useEnergyUnits } from "@/hooks/useEnergyUnits";
import { fmtNumber, fmtTime } from "@/lib/format";
import type { BilledUnits, BillPeriod, DayUnits, EnergySummary, MeterAssignment, MeterBalance, MeterNumber, MeterReading, MetersNow } from "@/lib/types";
import { cn } from "@/lib/utils";

const GRID_FILL: Record<"1" | "2" | "none", string> = {
  "1": "var(--viz-meter-1)",
  "2": "var(--viz-meter-2)",
  none: "var(--viz-unassigned)",
};
const SOLAR_FILL = "var(--viz-accent)";
const BATTERY_FILL = "var(--viz-context-strong)";
const READING_DAYS = Array.from({ length: 28 }, (_, index) => index + 1);
const METERS = [1, 2] as const;
const METER_KEY = { 1: "meter1_kwh", 2: "meter2_kwh" } as const;

const FIELD =
  "h-7 rounded-md border border-border bg-background px-1.5 text-xs pointer-coarse:h-10 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";
const TEXT_BUTTON =
  "rounded px-1 font-medium text-foreground underline underline-offset-2 hover:text-foreground/80 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

type Editor = "readings" | "assign" | null;

function fmtUnits(kwh: number): string {
  if (kwh >= 100) return fmtNumber(kwh, 0);
  return fmtNumber(kwh, kwh >= 10 ? 1 : 2);
}

function unitsLabel(kwh: number): string {
  return `${fmtUnits(kwh)} ${Math.abs(kwh - 1) < 0.005 ? "unit" : "units"}`;
}

function parseDay(date: string): Date {
  const [year, month, day] = date.split("-").map(Number);
  return new Date(year, month - 1, day);
}

function dayMonth(date: string): string {
  return parseDay(date.slice(0, 10)).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

function readingMoment(at: string): string {
  return `${dayMonth(at)}, ${fmtTime(at)}`;
}

function nowHHMM(): string {
  const now = new Date();
  return `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
}

function ordinal(day: number): string {
  if (day % 10 === 1 && day !== 11) return `${day}st`;
  if (day % 10 === 2 && day !== 12) return `${day}nd`;
  if (day % 10 === 3 && day !== 13) return `${day}rd`;
  return `${day}th`;
}

function localInputValue(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

interface MeterCheck {
  meters: MetersNow;
  forecastTotal: number | null;
  /** A meter whose reading rose although the log has next to nothing on it — usually a pro-rata (estimated) bill reading. */
  suspect: { meter: MeterNumber; used: number } | null;
  extraKwh: number;
  hours: number;
  suggestedWatts: number | null;
}

function meterCheck(summary: EnergySummary): MeterCheck | null {
  const { meters, bill } = summary;
  if (!meters || !bill) return null;
  const { projected, so_far } = bill;
  const remaining = (key: "meter1_kwh" | "meter2_kwh" | "unassigned_kwh") =>
    projected ? Math.max(0, projected[key] - so_far[key]) * meters.factor : null;
  const forecastOf = (meter: MeterNumber) => {
    const rest = remaining(METER_KEY[meter]);
    return rest == null ? null : meters[METER_KEY[meter]] + rest;
  };
  const forecast = { 1: forecastOf(1), 2: forecastOf(2) };
  const forecastTotal =
    forecast[1] != null && forecast[2] != null ? forecast[1] + forecast[2] + (remaining("unassigned_kwh") ?? 0) : null;

  const used = { 1: meters.latest.meter1 - meters.start.meter1, 2: meters.latest.meter2 - meters.start.meter2 };
  const suspectMeter = METERS.find((meter) => so_far[METER_KEY[meter]] < 0.05 && used[meter] >= so_far.unassigned_kwh + 1);
  const suspect = suspectMeter ? { meter: suspectMeter, used: used[suspectMeter] } : null;

  const hours = (Date.parse(meters.latest.at) - Date.parse(meters.start.at)) / 3_600_000;
  const extraKwh = used[1] + used[2] - meters.counted_kwh;
  const watts = Math.round(Math.min(100, Math.max(0, summary.standby_w + (extraKwh * 1000) / hours)));
  const suggestedWatts =
    suspect == null && hours >= 72 && Math.abs(meters.factor - 1) >= 0.03 && watts !== summary.standby_w ? watts : null;

  return { meters, forecastTotal, suspect, extraKwh, hours, suggestedWatts };
}

export function UnitsCard() {
  const { summary, saveReading, saveStandby, switchMeter, assignRange, removeAssignment, saveReadings, removeReading, saveBillUnits } =
    useEnergyUnits();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editor, setEditor] = useState<Editor>(null);
  const settingsRef = useRef<HTMLDivElement>(null);

  if (!summary) {
    return <section className="h-64 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }

  const check = meterCheck(summary);
  const latestReading = summary.latest_reading;
  const unassigned = summary.bill?.so_far.unassigned_kwh ?? 0;

  const openEditor = (next: Exclude<Editor, null>) => {
    setSettingsOpen(true);
    setEditor(next);
    requestAnimationFrame(() => settingsRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" }));
  };

  const settingsSummary = [
    latestReading ? `Meters checked ${dayMonth(latestReading.at)}` : "Meters not checked yet",
    unassigned >= 0.005 ? `${unitsLabel(unassigned)} not on a meter` : null,
    `read on the ${ordinal(summary.bill_reading_day)}`,
    `inverter ${fmtNumber(summary.standby_w, 0)} W`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card" aria-labelledby="units-title">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2 border-b px-4 sm:px-5 py-3">
        <div>
          <h2 id="units-title" className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Electricity units
          </h2>
          <p className="text-micro text-muted-foreground">
            Taken from the grid · 1 unit = 1 kWh
            <StatusLine summary={summary} />
          </p>
        </div>
      </div>

      {!summary.configured ? (
        <p className="px-4 sm:px-5 py-6 text-sm text-muted-foreground">Add your Solar of Things login in Settings to count units.</p>
      ) : (
        <>
          <PlanBand
            summary={summary}
            check={check}
            pending={switchMeter.isPending}
            error={switchMeter.error ? String(switchMeter.error) : null}
            onSwitch={(meter, at) => switchMeter.mutateAsync({ meter, at })}
          />
          <div className="grid grid-cols-1 lg:grid-cols-3 lg:divide-x">
            <TodayColumn summary={summary} />
            <BillsColumn
              summary={summary}
              check={check}
              onCheckReadings={() => openEditor("readings")}
              billUnits={{
                pending: saveBillUnits.isPending,
                error: saveBillUnits.error ? String(saveBillUnits.error) : null,
                onSave: (meter1, meter2) => saveBillUnits.mutateAsync({ meter1, meter2 }),
              }}
            />
            <RecentColumn days={summary.recent_days} billStart={summary.bill?.start ?? null} />
          </div>
          <AttentionStrip
            summary={summary}
            check={check}
            pending={assignRange.isPending}
            error={assignRange.error && editor !== "assign" ? String(assignRange.error) : null}
            onAssign={() => openEditor("assign")}
            onQuickAssign={(from, to, meter) => assignRange.mutate({ from, to, meter })}
            onCheckReadings={() => openEditor("readings")}
          />
        </>
      )}

      <div ref={settingsRef} className="scroll-mb-4 border-t">
        <Accordion value={settingsOpen ? ["settings"] : []} onValueChange={(value) => setSettingsOpen(value.includes("settings"))}>
          <AccordionItem value="settings">
            <AccordionTrigger className="items-center px-4 sm:px-5 py-3 hover:no-underline">
              <span className="flex flex-1 flex-wrap items-baseline gap-x-3 gap-y-0.5 pr-3">
                <span className="text-xs font-semibold text-foreground">Meter readings &amp; settings</span>
                <span className="text-micro font-normal tabular-nums text-muted-foreground">{settingsSummary}</span>
              </span>
            </AccordionTrigger>
            <AccordionContent className="h-auto px-4 sm:px-5 pb-4">
              <dl className="grid grid-cols-1 gap-x-6 gap-y-5 sm:grid-cols-[10rem_minmax(0,1fr)]">
                <SettingRow label="Meter readings" hint="Corrects the counts">
                  <MeterReadings
                    open={editor === "readings"}
                    onOpenChange={(open) => setEditor(open ? "readings" : null)}
                    billStart={summary.bill?.start ?? null}
                    opening={summary.opening_reading}
                    latest={summary.latest_reading}
                    pending={saveReadings.isPending || removeReading.isPending}
                    error={saveReadings.error ? String(saveReadings.error) : removeReading.error ? String(removeReading.error) : null}
                    onSave={(baseAt, base1, base2, at, meter1, meter2) => saveReadings.mutateAsync({ baseAt, base1, base2, at, meter1, meter2 })}
                    onRemove={(at) => removeReading.mutate(at)}
                  />
                </SettingRow>
                <SettingRow label="Past units" hint="Which meter the grid was on">
                  <AssignPast
                    open={editor === "assign"}
                    onOpenChange={(open) => setEditor(open ? "assign" : null)}
                    billStart={summary.bill?.start ?? null}
                    assignments={summary.assignments}
                    unassigned={unassigned}
                    pending={assignRange.isPending || removeAssignment.isPending}
                    error={assignRange.error ? String(assignRange.error) : removeAssignment.error ? String(removeAssignment.error) : null}
                    onAssign={(from, to, meter) => assignRange.mutateAsync({ from, to, meter })}
                    onRemove={(from) => removeAssignment.mutate(from)}
                  />
                </SettingRow>
                <SettingRow label="Reading day" hint="When the meters are read">
                  <ReadingPicker
                    day={summary.bill_reading_day}
                    time={summary.bill_reading_time}
                    pending={saveReading.isPending}
                    error={saveReading.error ? String(saveReading.error) : null}
                    onSave={(day, time) => saveReading.mutate({ day, time })}
                  />
                </SettingRow>
                <SettingRow label="Inverter's own draw" hint="Grid power its log doesn't show">
                  <div className="space-y-2">
                    <StandbyField
                      watts={summary.standby_w}
                      pending={saveStandby.isPending}
                      error={saveStandby.error ? String(saveStandby.error) : null}
                      onSave={(watts) => saveStandby.mutate(watts)}
                    />
                    <CalibrationHint
                      check={check}
                      gapHours={summary.bill ? summary.bill.elapsed_hours - summary.bill.log_hours : 0}
                      pending={saveStandby.isPending}
                      onUse={(watts) => saveStandby.mutate(watts)}
                      onFixReadings={() => openEditor("readings")}
                    />
                  </div>
                </SettingRow>
              </dl>
              <p className="mt-5 max-w-3xl border-t pt-3 text-micro leading-relaxed text-muted-foreground">
                Counted from the inverter's 5-minute log: the house load that solar and the battery didn't cover while the grid was on, plus
                the inverter's own draw from the grid. Loads not wired through the inverter, and grid charging of the battery, aren't counted.
                Meter readings rescale the counts to match what the meters really show.
              </p>
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      </div>
    </section>
  );
}

function SettingRow({ label, hint, children }: { label: string; hint: string; children: ReactNode }) {
  return (
    <div className="contents">
      <dt className="text-xs font-medium text-foreground sm:pt-1.5">
        {label}
        <span className="block text-micro font-normal text-muted-foreground">{hint}</span>
      </dt>
      <dd className="min-w-0 text-xs text-muted-foreground">{children}</dd>
    </div>
  );
}

function ErrorText({ error }: { error: string | null }) {
  return error ? (
    <p role="alert" className="text-destructive">
      {error}
    </p>
  ) : null;
}

function ReadingPair({ label, reading }: { label: string; reading: MeterReading | undefined }) {
  return (
    <span className="inline-flex flex-col">
      <span className="text-micro text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums text-foreground">{reading ? `${reading.meter1} · ${reading.meter2}` : "–"}</span>
    </span>
  );
}

function MeterReadings({
  open,
  onOpenChange,
  billStart,
  opening,
  latest,
  pending,
  error,
  onSave,
  onRemove,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  billStart: string | null;
  opening: MeterReading | null;
  latest: MeterReading | null;
  pending: boolean;
  error: string | null;
  onSave: (baseAt: string, base1: number, base2: number, at: string, meter1: number, meter2: number) => Promise<unknown>;
  onRemove: (at: string) => void;
}) {
  const initial = () => ({
    baseAt: (opening?.at ?? billStart ?? "").slice(0, 16),
    base1: opening ? String(opening.meter1) : "",
    base2: opening ? String(opening.meter2) : "",
    at: latest ? latest.at.slice(0, 16) : localInputValue(new Date()),
    meter1: latest ? String(latest.meter1) : "",
    meter2: latest ? String(latest.meter2) : "",
  });
  const [draft, setDraft] = useState(initial);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setDraft(initial());
  }
  const values = [draft.base1, draft.base2, draft.meter1, draft.meter2].map((text) => (text.trim() === "" ? NaN : Number(text)));
  const ready = draft.baseAt !== "" && draft.at !== "" && values.every((value) => Number.isFinite(value) && value >= 0);
  const backwards = ready && (values[2] < values[0] || values[3] < values[1]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready || backwards) return;
    const [base1, base2, meter1, meter2] = values;
    try {
      await onSave(draft.baseAt, base1, base2, draft.at, meter1, meter2);
      onOpenChange(false);
    } catch {
      return;
    }
  };

  const input = (key: "base1" | "base2" | "meter1" | "meter2", label: string) => (
    <input
      type="number"
      inputMode="decimal"
      min={0}
      step="any"
      aria-label={label}
      className={cn(FIELD, "w-24 pointer-coarse:w-28")}
      value={draft[key]}
      onChange={(event) => setDraft({ ...draft, [key]: event.target.value })}
    />
  );

  if (open) {
    return (
      <form onSubmit={submit} className="space-y-3">
        <div className="grid w-fit grid-cols-[auto_auto_auto_auto] items-center gap-x-3 gap-y-2">
          <span />
          <span className="text-micro font-medium uppercase tracking-wide">Taken</span>
          <span className="text-micro font-medium uppercase tracking-wide">Meter 1</span>
          <span className="text-micro font-medium uppercase tracking-wide">Meter 2</span>
          <span>Bill's meter photo</span>
          <input
            type="datetime-local"
            aria-label="When the bill's meter photo was taken"
            className={FIELD}
            value={draft.baseAt}
            onChange={(event) => setDraft({ ...draft, baseAt: event.target.value })}
          />
          {input("base1", "Meter 1 in the bill's photo")}
          {input("base2", "Meter 2 in the bill's photo")}
          <span>Latest</span>
          <input
            type="datetime-local"
            aria-label="When you read the meters"
            className={FIELD}
            value={draft.at}
            max={localInputValue(new Date())}
            onChange={(event) => setDraft({ ...draft, at: event.target.value })}
          />
          {input("meter1", "Meter 1 at the latest reading")}
          {input("meter2", "Meter 2 at the latest reading")}
        </div>
        <p className="max-w-lg text-micro leading-snug">
          A “Pro-Rata” bill's Present Reading is an estimate for the reading date. Use the number on the meter in the bill's photo, and the
          date and time printed above the photo (the meter reader's visit). Previous + Outstanding units gives the same number. Type the
          decimals too.
        </p>
        {backwards ? (
          <p role="alert" className="text-destructive">
            The latest reading can't be lower than the photo.
          </p>
        ) : null}
        <div className="flex flex-wrap items-center gap-2">
          <Button type="submit" size="sm" disabled={pending || !ready || backwards}>
            Save readings
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
        </div>
        <ErrorText error={error} />
      </form>
    );
  }

  return (
    <div className="space-y-2">
      {latest ? (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <ReadingPair label={opening ? readingMoment(opening.at) : "Bill's meter photo"} reading={opening ?? undefined} />
          <span aria-hidden className="text-muted-foreground">
            →
          </span>
          <ReadingPair label={readingMoment(latest.at)} reading={latest} />
          <span className="flex items-center gap-1">
            <Button type="button" variant="outline" size="sm" onClick={() => onOpenChange(true)} disabled={!billStart}>
              Edit
            </Button>
            <button type="button" disabled={pending} onClick={() => onRemove(latest.at)} className={cn(TEXT_BUTTON, "hover:text-destructive")}>
              Remove
            </button>
          </span>
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <span>Type in what both meters show, and the counts are corrected to match.</span>
          <Button type="button" variant="outline" size="sm" onClick={() => onOpenChange(true)} disabled={!billStart}>
            Enter readings
          </Button>
        </div>
      )}
      <ErrorText error={error} />
    </div>
  );
}

function AssignPast({
  open,
  onOpenChange,
  billStart,
  assignments,
  unassigned,
  pending,
  error,
  onAssign,
  onRemove,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  billStart: string | null;
  assignments: MeterAssignment[];
  unassigned: number;
  pending: boolean;
  error: string | null;
  onAssign: (from: string, to: string, meter: MeterNumber) => Promise<unknown>;
  onRemove: (from: string) => void;
}) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [meter, setMeter] = useState<MeterNumber>(2);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setFrom(billStart ? billStart.slice(0, 16) : "");
      setTo(localInputValue(new Date()));
    }
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!from || !to) return;
    try {
      await onAssign(from, to, meter);
      onOpenChange(false);
    } catch {
      return;
    }
  };

  return (
    <div className="space-y-2">
      {assignments.length > 0 ? (
        <ul className="space-y-1">
          {assignments.map((assignment) => (
            <li key={assignment.from} className="flex flex-wrap items-center gap-2 tabular-nums">
              <span className="size-2 shrink-0 rounded-full" style={{ background: GRID_FILL[String(assignment.meter) as "1" | "2"] }} aria-hidden />
              <span className="text-foreground">
                {readingMoment(assignment.from)} – {readingMoment(assignment.to)}
              </span>
              <span>on Meter {assignment.meter}</span>
              <button type="button" disabled={pending} onClick={() => onRemove(assignment.from)} className={cn(TEXT_BUTTON, "hover:text-destructive")}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {open ? (
        <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
          <label className="inline-flex items-center gap-1.5">
            From
            <input type="datetime-local" className={FIELD} value={from} max={to || undefined} onChange={(event) => setFrom(event.target.value)} />
          </label>
          <label className="inline-flex items-center gap-1.5">
            to
            <input type="datetime-local" className={FIELD} value={to} min={from || undefined} onChange={(event) => setTo(event.target.value)} />
          </label>
          <label className="inline-flex items-center gap-1.5">
            the grid was on
            <select className={FIELD} value={meter} onChange={(event) => setMeter(Number(event.target.value) as MeterNumber)}>
              <option value={1}>Meter 1</option>
              <option value={2}>Meter 2</option>
            </select>
          </label>
          <Button type="submit" size="sm" disabled={pending || !from || !to}>
            Assign
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
        </form>
      ) : (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          {unassigned >= 0.005 ? (
            <span className="tabular-nums text-foreground">{unitsLabel(unassigned)} this bill aren't on a meter yet.</span>
          ) : assignments.length === 0 ? (
            <span>Every unit this bill is on a meter.</span>
          ) : null}
          <Button type="button" variant="outline" size="sm" onClick={() => onOpenChange(true)}>
            Assign a time range
          </Button>
        </div>
      )}
      <ErrorText error={error} />
    </div>
  );
}

function ReadingPicker({
  day,
  time,
  pending,
  error,
  onSave,
}: {
  day: number;
  time: string;
  pending: boolean;
  error: string | null;
  onSave: (day: number, time: string) => void;
}) {
  const [draftTime, setDraftTime] = useState(time);
  const [shownTime, setShownTime] = useState(time);
  if (time !== shownTime) {
    setShownTime(time);
    setDraftTime(time);
  }
  const saveTime = () => {
    if (draftTime && draftTime !== time) onSave(day, draftTime);
  };
  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <span>The</span>
        <select aria-label="Reading day" className={FIELD} value={day} disabled={pending} onChange={(event) => onSave(Number(event.target.value), time)}>
          {READING_DAYS.map((option) => (
            <option key={option} value={option}>
              {ordinal(option)}
            </option>
          ))}
        </select>
        <span>of each month, at</span>
        <input
          type="time"
          aria-label="Reading time"
          className={FIELD}
          value={draftTime}
          disabled={pending}
          onChange={(event) => setDraftTime(event.target.value)}
          onBlur={saveTime}
          onKeyDown={(event) => {
            if (event.key === "Enter") saveTime();
          }}
        />
      </div>
      <ErrorText error={error} />
    </div>
  );
}

function StandbyField({
  watts,
  pending,
  error,
  onSave,
}: {
  watts: number;
  pending: boolean;
  error: string | null;
  onSave: (watts: number) => void;
}) {
  const [draft, setDraft] = useState(String(watts));
  const [shown, setShown] = useState(watts);
  if (watts !== shown) {
    setShown(watts);
    setDraft(String(watts));
  }
  const save = () => {
    const value = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(value)) {
      setDraft(String(watts));
      return;
    }
    if (value !== watts) onSave(value);
  };
  return (
    <div className="space-y-1">
      <label className="inline-flex items-center gap-2">
        <input
          type="number"
          inputMode="decimal"
          aria-label="Inverter's own draw in watts"
          min={0}
          max={100}
          step={1}
          value={draft}
          disabled={pending}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={save}
          onKeyDown={(event) => {
            if (event.key === "Enter") save();
          }}
          className={cn(FIELD, "w-16 pointer-coarse:w-20")}
        />
        W
      </label>
      <ErrorText error={error} />
    </div>
  );
}

function CalibrationHint({
  check,
  gapHours,
  pending,
  onUse,
  onFixReadings,
}: {
  check: MeterCheck | null;
  gapHours: number;
  pending: boolean;
  onUse: (watts: number) => void;
  onFixReadings: () => void;
}) {
  if (!check) {
    return <p className="max-w-md leading-snug">Enter meter readings and the app will tell you whether this needs changing.</p>;
  }
  if (check.suspect) {
    return (
      <p className="flex max-w-md items-start gap-1.5 leading-snug">
        <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
        <span>
          Leave this as it is for now — Meter {check.suspect.meter}'s readings don't line up with the log yet.{" "}
          <button type="button" onClick={onFixReadings} className={TEXT_BUTTON}>
            Check the readings
          </button>
        </span>
      </p>
    );
  }
  if (Math.abs(check.meters.factor - 1) < 0.03) {
    return (
      <p className="flex items-center gap-1.5">
        <CircleCheck className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
        The app matches your meters.
      </p>
    );
  }
  if (check.suggestedWatts == null) {
    return <p className="max-w-md leading-snug">The app is off from your meters, but it's too early to tell why. Check again after a few more days.</p>;
  }
  const more = check.extraKwh > 0;
  return (
    <div className="max-w-md space-y-2 leading-snug">
      <p>
        Your meters show <span className="font-medium text-foreground">{unitsLabel(Math.abs(check.extraKwh))}</span> {more ? "more" : "fewer"}{" "}
        than the app counted since {dayMonth(check.meters.start.at)}. About{" "}
        <span className="font-medium text-foreground">{check.suggestedWatts} W</span> would close the gap
        {gapHours >= 1 ? ` — some of it may be the ${fmtNumber(gapHours, 1)} h missing from the log` : ""}.
      </p>
      <Button type="button" variant="outline" size="sm" disabled={pending} onClick={() => onUse(check.suggestedWatts!)}>
        Use {check.suggestedWatts} W
      </Button>
    </div>
  );
}

function StatusLine({ summary }: { summary: EnergySummary }) {
  if (summary.backfill) {
    return (
      <span className="tabular-nums" role="status">
        {" "}
        · Filling in past days… {summary.backfill.done} of {summary.backfill.total}
      </span>
    );
  }
  if (summary.error) {
    return (
      <span className="text-destructive" role="alert">
        {" "}
        · {summary.error}
      </span>
    );
  }
  return summary.updated_at ? <span className="tabular-nums"> · updated {fmtTime(summary.updated_at)}</span> : null;
}

function MeterRegister({ kwh }: { kwh: number }) {
  const [whole, fraction] = Math.max(0, kwh).toFixed(2).split(".");
  const digits = whole.padStart(4, "0");
  const firstSignificant = digits.search(/[1-9]/);
  const leading = firstSignificant === -1 ? digits.length - 1 : firstSignificant;
  return (
    <div className="mt-2 flex items-end gap-2">
      <div
        role="img"
        aria-label={`${unitsLabel(kwh)} from the grid today`}
        className="flex items-end gap-[3px] rounded-md bg-neutral-900 p-[3px] shadow-inner ring-1 ring-black/10 dark:bg-black/70 dark:ring-white/10"
      >
        {digits.split("").map((digit, index) => (
          <Digit key={`whole-${index}`} digit={digit} dim={index < leading} />
        ))}
        <span aria-hidden className="self-end px-px pb-1 text-xl font-bold leading-none text-neutral-100 sm:text-2xl">
          .
        </span>
        {fraction.split("").map((digit, index) => (
          <Digit key={`fraction-${index}`} digit={digit} fraction />
        ))}
      </div>
      <span className="pb-1 text-sm font-medium text-muted-foreground">units</span>
    </div>
  );
}

function Digit({ digit, dim = false, fraction = false }: { digit: string; dim?: boolean; fraction?: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid h-10 w-7 place-items-center rounded-[3px] bg-linear-to-b from-white/10 via-transparent to-white/5 text-xl sm:h-11 sm:w-8 sm:text-2xl font-semibold leading-none tabular-nums",
        fraction ? "bg-[var(--viz-accent)] text-white" : dim ? "text-neutral-500" : "text-neutral-100",
      )}
    >
      {digit}
    </span>
  );
}

function TodayColumn({ summary }: { summary: EnergySummary }) {
  const today = summary.today;
  const yesterday = summary.yesterday;
  return (
    <div className="px-4 sm:px-5 py-4">
      <p className="flex items-baseline justify-between text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>Today so far</span>
        <span className="normal-case tracking-normal">from the inverter log</span>
      </p>
      {today ? (
        <>
          <MeterRegister kwh={today.grid_kwh} />
          <p className="mt-4 text-sm tabular-nums">
            The house used <span className="font-semibold">{unitsLabel(today.house_kwh)}</span>
          </p>
          <ShareBar day={today} />
        </>
      ) : summary.error ? (
        <p className="mt-2 text-sm text-muted-foreground">Today's numbers show up once the inverter cloud answers.</p>
      ) : (
        <div className="mt-2 space-y-2" role="status">
          <Skeleton className="h-12 w-52" />
          <p className="text-xs text-muted-foreground">Fetching today's log from the inverter cloud…</p>
        </div>
      )}
      <p className="mt-4 border-t pt-3 text-xs tabular-nums text-muted-foreground">
        {yesterday && yesterday.house_kwh > 0 ? (
          <>
            Yesterday <span className="font-semibold text-foreground">{unitsLabel(yesterday.grid_kwh)}</span> from the grid ·{" "}
            {fmtUnits(Math.max(0, yesterday.house_kwh - yesterday.grid_kwh))} from solar and the battery
          </>
        ) : (
          "Yesterday isn't in the records yet."
        )}
      </p>
    </div>
  );
}

function ShareBar({ day }: { day: DayUnits }) {
  const grid = [
    { label: "Grid · Meter 1", kwh: day.meter1_kwh, fill: GRID_FILL["1"] },
    { label: "Grid · Meter 2", kwh: day.meter2_kwh, fill: GRID_FILL["2"] },
    { label: "Grid · not assigned", kwh: day.unassigned_kwh, fill: GRID_FILL.none },
  ].filter((part) => part.kwh >= 0.005);
  const parts = [
    ...(grid.length > 0 ? grid : [{ label: "Grid", kwh: 0, fill: GRID_FILL.none }]),
    { label: "Solar", kwh: day.solar_kwh, fill: SOLAR_FILL },
    { label: "Battery", kwh: day.battery_kwh, fill: BATTERY_FILL },
  ];
  const sum = parts.reduce((total, part) => total + part.kwh, 0);
  const total = sum > 0 ? sum : 1;
  return (
    <>
      <div className="mt-2 flex h-2 gap-px overflow-hidden rounded-full bg-muted" aria-hidden>
        {parts
          .filter((part) => part.kwh >= 0.005)
          .map((part) => (
            <div key={part.label} style={{ background: part.fill, width: `${(part.kwh / total) * 100}%` }} />
          ))}
      </div>
      <dl className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs tabular-nums text-muted-foreground">
        {parts.map((part) => (
          <div key={part.label} className="inline-flex items-center gap-1.5">
            <span className="size-2.5 rounded-xs" style={{ background: part.fill }} aria-hidden />
            <dt>{part.label}</dt>
            <dd className="font-medium text-foreground">{fmtUnits(part.kwh)}</dd>
          </div>
        ))}
      </dl>
    </>
  );
}

function MeterSwitcher({
  active,
  pending,
  error,
  onSwitch,
}: {
  active: MeterNumber | null;
  pending: boolean;
  error: string | null;
  onSwitch: (meter: MeterNumber, at: string | null) => Promise<unknown>;
}) {
  const [target, setTarget] = useState<MeterNumber | null>(null);
  const [time, setTime] = useState("");
  const [openedAt, setOpenedAt] = useState("");

  const begin = (meter: MeterNumber) => {
    if (meter === active) return;
    const now = nowHHMM();
    setTarget(meter);
    setTime(now);
    setOpenedAt(now);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (target == null) return;
    try {
      await onSwitch(target, time && time !== openedAt ? time : null);
      setTarget(null);
    } catch {
      return;
    }
  };

  return (
    <div className="flex flex-col items-start gap-1 sm:items-end">
      <div className="flex items-center gap-2 text-xs">
        <span className="text-muted-foreground">Grid from</span>
        <ToggleGroup
          aria-label="Meter the changeover switch is on"
          value={active != null ? [String(active)] : []}
          onValueChange={(next) => {
            const meter = Number(next[0]);
            if (meter === 1 || meter === 2) begin(meter);
          }}
          disabled={pending}
          spacing={0.5}
          className="rounded-md border border-border p-0.5"
        >
          {METERS.map((meter) => (
            <ToggleGroupItem key={meter} value={String(meter)} size="sm" className="h-auto gap-1.5 rounded px-2.5 py-1 font-medium">
              <span className="size-2 rounded-full" style={{ background: GRID_FILL[String(meter) as "1" | "2"] }} aria-hidden />
              Meter {meter}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      {target != null ? (
        <form onSubmit={submit} className="flex flex-wrap items-center gap-2 text-xs sm:justify-end">
          <label className="inline-flex items-center gap-1.5">
            Switched to Meter {target} at
            <input type="time" value={time} max={openedAt} onChange={(event) => setTime(event.target.value)} className={FIELD} />
          </label>
          <Button type="submit" size="sm" disabled={pending}>
            Save
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => setTarget(null)}>
            Cancel
          </Button>
        </form>
      ) : null}
      {error ? (
        <p role="alert" className="text-micro text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function PlanBand({
  summary,
  check,
  pending,
  error,
  onSwitch,
}: {
  summary: EnergySummary;
  check: MeterCheck | null;
  pending: boolean;
  error: string | null;
  onSwitch: (meter: MeterNumber, at: string | null) => Promise<unknown>;
}) {
  const { balance, bill, active_meter: active } = summary;
  const used = (meter: MeterNumber) => (check ? check.meters[METER_KEY[meter]] : (bill?.so_far[METER_KEY[meter]] ?? 0));
  return (
    <div className="space-y-4 border-b px-4 sm:px-5 py-4">
      <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <PlanHeadline
          balance={balance}
          active={active}
          since={summary.active_since}
          end={bill?.end ?? null}
          used={used}
          pending={pending}
          onSwitched={(meter) => void onSwitch(meter, null)}
        />
        <MeterSwitcher active={active} pending={pending} error={error} onSwitch={onSwitch} />
      </div>
      {bill ? <MeterBars bill={bill} used={used} balance={balance} active={active} /> : null}
    </div>
  );
}

function fmtOne(kwh: number): string {
  return Math.max(0, kwh).toFixed(1);
}

function sinceLabel(at: string): string {
  return new Date(at).toDateString() === new Date().toDateString() ? fmtTime(at) : dayMonth(at);
}

function roughWait(at: string): string {
  const hours = (Date.parse(at) - Date.now()) / 3_600_000;
  if (hours < 1.5) return "within the hour or so";
  if (hours < 36) return `about ${Math.round(hours)} hours`;
  return `about ${Math.round(hours / 24)} days`;
}

function PlanHeadline({
  balance,
  active,
  since,
  end,
  used,
  pending,
  onSwitched,
}: {
  balance: MeterBalance | null;
  active: MeterNumber | null;
  since: string | null;
  end: string | null;
  used: (meter: MeterNumber) => number;
  pending: boolean;
  onSwitched: (meter: MeterNumber) => void;
}) {
  if (active == null) {
    return <p className="text-sm text-muted-foreground">Tap the meter the changeover switch is on, and this will say when to swap.</p>;
  }
  const onSince = since ? `On Meter ${active} since ${sinceLabel(since)}` : null;
  if (!balance || !end) {
    return (
      <div className="space-y-1">
        <p className="text-sm text-muted-foreground">A switch plan shows up once this bill has a few hours of data.</p>
        {onSince ? <p className="text-xs text-muted-foreground">{onSince}</p> : null}
      </div>
    );
  }
  const to = balance.switch_to;
  const due = to != null && balance.switch_at != null && new Date(balance.switch_at) <= new Date();
  const headline =
    to == null ? `Stay on Meter ${active} until the reading` : due ? `Switch to Meter ${to} now` : `Switch to Meter ${to} ${switchMoment(balance.switch_at!)}`;
  const even = Math.abs(balance.meter1_kwh - balance.meter2_kwh) < 1;
  const detail =
    to == null
      ? even
        ? `both meters end near ${unitsLabel((balance.meter1_kwh + balance.meter2_kwh) / 2)} on ${dayMonth(end)}`
        : `Meter ${3 - active} is already ahead, so Meter ${active} takes the rest of the bill`
      : due
        ? `Meter ${active} has its share; Meter ${to} takes the rest, about ${unitsLabel(balance[METER_KEY[to]] - used(to))}`
        : `stay for ~${unitsLabel(balance[METER_KEY[active]] - used(active))} more (${roughWait(balance.switch_at!)}), then Meter ${to} takes the rest`;
  const subline = [onSince, detail].filter(Boolean).join(" · ");
  return (
    <div className="min-w-0 space-y-1" role="status">
      <p className="flex items-center gap-2 text-base font-semibold tabular-nums">
        <ArrowLeftRight className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        {headline}
      </p>
      <p className="pl-6 text-xs tabular-nums text-muted-foreground">{subline.charAt(0).toUpperCase() + subline.slice(1)}</p>
      {due && to != null ? (
        <Button type="button" size="sm" variant="outline" className="mt-1 ml-6" disabled={pending} onClick={() => onSwitched(to)}>
          I've switched to Meter {to}
        </Button>
      ) : null}
    </div>
  );
}

const BAR_ROW = "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 sm:grid-cols-[8rem_minmax(0,1fr)_7.5rem]";

function MeterBars({
  bill,
  used,
  balance,
  active,
}: {
  bill: BillPeriod;
  used: (meter: MeterNumber) => number;
  balance: MeterBalance | null;
  active: MeterNumber | null;
}) {
  const finish = (meter: MeterNumber) => (balance ? balance[METER_KEY[meter]] : null);
  const target = balance?.target_kwh ?? null;
  const scale = Math.max(1, target ?? 0, ...METERS.map((meter) => Math.max(used(meter), finish(meter) ?? 0))) * 1.05;
  const share = (kwh: number) => Math.min(100, (Math.max(0, kwh) / scale) * 100);
  const pct = (kwh: number) => `${share(kwh)}%`;
  const endLabel = dayMonth(bill.end);

  return (
    <div className="space-y-2 text-sm tabular-nums">
      {target != null ? (
        <div className={BAR_ROW} aria-hidden>
          <span className="hidden sm:block" />
          <div className="relative col-span-2 h-4 sm:col-span-1">
            <span
              className={cn(
                "absolute bottom-0 whitespace-nowrap text-micro text-muted-foreground",
                share(target) > 85 ? "-translate-x-full" : share(target) > 15 && "-translate-x-1/2",
              )}
              style={{ left: pct(target) }}
            >
              even {fmtOne(target)}
            </span>
          </div>
        </div>
      ) : null}
      {METERS.map((meter) => {
        const fill = GRID_FILL[String(meter) as "1" | "2"];
        const now = used(meter);
        const end = finish(meter);
        const planned = end != null && end - now >= 0.05;
        const label = `Meter ${meter}: ${unitsLabel(now)} so far${planned ? `, about ${unitsLabel(end)} by ${endLabel} on the plan` : ""}${
          target != null ? `; an even share is ${unitsLabel(target)}` : ""
        }`;
        return (
          <div key={meter} className={BAR_ROW}>
            <span className="order-1 inline-flex items-center gap-2 text-muted-foreground">
              <span className="size-2 shrink-0 rounded-full" style={{ background: fill }} aria-hidden />
              Meter {meter}
              {meter === active ? <span className="text-micro">· on now</span> : null}
            </span>
            <span className="order-2 text-right sm:order-3">
              <span className="font-semibold">{fmtOne(now)}</span>
              {planned ? <span className="text-xs text-muted-foreground"> → {fmtOne(end)}</span> : null}
            </span>
            <div className="relative order-3 col-span-2 h-2.5 rounded-full bg-muted sm:order-2 sm:col-span-1" role="img" aria-label={label}>
              {planned ? <div className="absolute inset-y-0 left-0 rounded-full border" style={{ width: pct(end), borderColor: fill }} /> : null}
              <div className="absolute inset-y-0 left-0 rounded-full" style={{ width: pct(now), background: fill }} />
              {target != null ? <div className="absolute -inset-y-1 w-px bg-foreground/60" style={{ left: pct(target) }} aria-hidden /> : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function BillsColumn({
  summary,
  check,
  onCheckReadings,
  billUnits,
}: {
  summary: EnergySummary;
  check: MeterCheck | null;
  onCheckReadings: () => void;
  billUnits: { pending: boolean; error: string | null; onSave: (meter1: number, meter2: number) => Promise<unknown> };
}) {
  return (
    <div className="space-y-5 border-t px-4 sm:px-5 py-4 lg:border-t-0">
      {summary.bill ? (
        <CurrentBill period={summary.bill} check={check} onCheckReadings={onCheckReadings} />
      ) : (
        <p className="text-sm text-muted-foreground">Set the reading day below to start counting this bill.</p>
      )}
      {summary.previous_bill ? <LastBill period={summary.previous_bill} billed={summary.previous_billed} {...billUnits} /> : null}
    </div>
  );
}

function SectionHeading({ title, period }: { title: string; period: BillPeriod }) {
  return (
    <p className="flex flex-wrap items-baseline justify-between gap-x-2 text-micro font-medium uppercase tracking-wide text-muted-foreground">
      <span>{title}</span>
      <span className="normal-case tracking-normal tabular-nums">
        {dayMonth(period.start)} → {dayMonth(period.end)}
      </span>
    </p>
  );
}

function CurrentBill({ period, check, onCheckReadings }: { period: BillPeriod; check: MeterCheck | null; onCheckReadings: () => void }) {
  const total = check ? check.meters.meter1_kwh + check.meters.meter2_kwh : period.so_far.grid_kwh;
  const forecastTotal = check ? check.forecastTotal : (period.projected?.grid_kwh ?? null);

  return (
    <div>
      <SectionHeading title="This bill" period={period} />
      <p className="mt-2 flex items-baseline gap-2 tabular-nums">
        <span className="text-2xl font-semibold">{fmtUnits(total)}</span>
        <span className="text-sm text-muted-foreground">units so far</span>
      </p>
      {forecastTotal != null && forecastTotal - total >= 0.05 ? (
        <p className="mt-0.5 text-xs tabular-nums text-muted-foreground">
          On pace for <span className="font-medium text-foreground">~{unitsLabel(forecastTotal)}</span> by {dayMonth(period.end)}
        </p>
      ) : null}
      {check ? (
        <>
          <p className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs tabular-nums text-muted-foreground">
            <CircleCheck className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
            Checked with your meters {readingMoment(check.meters.latest.at)}
            <button type="button" onClick={onCheckReadings} className={TEXT_BUTTON}>
              Update
            </button>
          </p>
          <MetersVsLog period={period} meters={check.meters} />
        </>
      ) : (
        <p className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
          Estimated from the inverter's log ·
          <button type="button" onClick={onCheckReadings} className={TEXT_BUTTON}>
            Check against your meters
          </button>
        </p>
      )}
    </div>
  );
}

function MetersVsLog({ period, meters }: { period: BillPeriod; meters: MetersNow }) {
  const log = period.so_far;
  const rows = [
    { label: "Meter 1", fill: GRID_FILL["1"], meter: meters.meter1_kwh, log: log.meter1_kwh },
    { label: "Meter 2", fill: GRID_FILL["2"], meter: meters.meter2_kwh, log: log.meter2_kwh },
    ...(log.unassigned_kwh >= 0.005 ? [{ label: "Not on a meter", fill: GRID_FILL.none, meter: null, log: log.unassigned_kwh }] : []),
  ];
  const meterTotal = meters.meter1_kwh + meters.meter2_kwh;
  const gap = meterTotal - log.grid_kwh;
  return (
    <div className="mt-3">
      <table className="w-full text-xs tabular-nums">
        <thead className="text-muted-foreground">
          <tr>
            <th scope="col" className="pb-1 text-left font-normal">
              <span className="sr-only">Meter</span>
            </th>
            <th scope="col" className="pb-1 text-right font-normal">
              Meters
            </th>
            <th scope="col" className="pb-1 text-right font-normal">
              Inverter log
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.label}>
              <th scope="row" className="py-0.5 text-left font-normal text-muted-foreground">
                <span className="inline-flex items-center gap-2">
                  <span className="size-2 shrink-0 rounded-full" style={{ background: row.fill }} aria-hidden />
                  {row.label}
                </span>
              </th>
              <td className="py-0.5 text-right font-medium">{row.meter != null ? fmtOne(row.meter) : "–"}</td>
              <td className="py-0.5 text-right">{fmtOne(row.log)}</td>
            </tr>
          ))}
          <tr className="border-t">
            <th scope="row" className="pt-1 text-left font-normal text-muted-foreground">
              Total
            </th>
            <td className="pt-1 text-right font-semibold">{fmtOne(meterTotal)}</td>
            <td className="pt-1 text-right font-semibold">{fmtOne(log.grid_kwh)}</td>
          </tr>
        </tbody>
      </table>
      {Math.abs(gap) >= 0.05 && meterTotal > 0 ? (
        <p className="mt-1.5 text-xs tabular-nums text-muted-foreground">
          The inverter log is {unitsLabel(Math.abs(gap))} ({fmtNumber((Math.abs(gap) / meterTotal) * 100, 0)}%) {gap > 0 ? "under" : "over"} the
          meters
        </p>
      ) : null}
    </div>
  );
}

function LastBill({
  period,
  billed: bill,
  pending,
  error,
  onSave,
}: {
  period: BillPeriod;
  billed: BilledUnits | null;
  pending: boolean;
  error: string | null;
  onSave: (meter1: number, meter2: number) => Promise<unknown>;
}) {
  const billed = bill ? { 1: bill.meter1, 2: bill.meter2 } : null;
  const counted = period.so_far.grid_kwh;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({ meter1: "", meter2: "" });
  const values = [draft.meter1, draft.meter2].map((text) => (text.trim() === "" ? NaN : Number(text)));
  const ready = values.every((value) => Number.isFinite(value) && value >= 0);
  const missing = period.days_total - period.days_with_data;

  const begin = () => {
    setDraft({ meter1: billed ? String(billed[1]) : "", meter2: billed ? String(billed[2]) : "" });
    setEditing(true);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    try {
      await onSave(values[0], values[1]);
      setEditing(false);
    } catch {
      return;
    }
  };

  const total = billed ? billed[1] + billed[2] : counted;
  const off = billed && total > 0 ? (total - counted) / total : null;

  return (
    <div className="border-t pt-4">
      <SectionHeading title="Last bill" period={period} />
      <p className="mt-1.5 flex flex-wrap items-baseline gap-x-3 gap-y-1 tabular-nums">
        <span className="text-base font-semibold">{unitsLabel(total)}</span>
        {billed ? <span className="text-xs text-muted-foreground">Meter 1 {fmtUnits(billed[1])} · Meter 2 {fmtUnits(billed[2])}</span> : null}
      </p>
      {editing ? (
        <form onSubmit={submit} className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span>Units on the bill</span>
          {METERS.map((meter) => {
            const key = meter === 1 ? "meter1" : "meter2";
            return (
              <label key={meter} className="inline-flex items-center gap-1.5">
                Meter {meter}
                <input
                  type="number"
                  inputMode="decimal"
                  min={0}
                  step="any"
                  className={cn(FIELD, "w-20 pointer-coarse:w-24")}
                  value={draft[key]}
                  onChange={(event) => setDraft({ ...draft, [key]: event.target.value })}
                />
              </label>
            );
          })}
          <Button type="submit" size="sm" disabled={pending || !ready}>
            Save
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(false)}>
            Cancel
          </Button>
        </form>
      ) : (
        <p className="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs tabular-nums text-muted-foreground">
          {billed ? (
            <>
              From your bill · the app counted {fmtUnits(counted)}
              {off != null && Math.abs(off) >= 0.03 ? ` (${fmtNumber(Math.abs(off) * 100, 0)}% ${off > 0 ? "low" : "high"})` : ""}
            </>
          ) : (
            "Counted by the app · compare with your bill"
          )}
          {missing > 0 ? ` · ${missing} ${missing === 1 ? "day has" : "days have"} no data` : ""}
          <button type="button" onClick={begin} className={TEXT_BUTTON}>
            {billed ? "Edit" : "Enter the bill's units"}
          </button>
        </p>
      )}
      <div className="mt-1 text-xs">
        <ErrorText error={error} />
      </div>
    </div>
  );
}

function switchMoment(at: string): string {
  const when = new Date(at);
  const today = new Date();
  const tomorrow = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1);
  const day =
    when.toDateString() === today.toDateString()
      ? "today"
      : when.toDateString() === tomorrow.toDateString()
        ? "tomorrow"
        : when.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
  return `${day} around ${fmtTime(at)}`;
}

function unassignedWindow(billStart: string, assignments: MeterAssignment[]): { from: string; to: string } | null {
  const from = assignments.reduce((start, assignment) => (assignment.from <= start && assignment.to > start ? assignment.to : start), billStart);
  const to = assignments
    .map((assignment) => assignment.from)
    .filter((at) => at > from)
    .sort()[0];
  return to ? { from, to } : null;
}

interface AttentionItem {
  key: string;
  text: ReactNode;
  actions: { label: string; onClick: () => void }[];
}

function AttentionStrip({
  summary,
  check,
  pending,
  error,
  onAssign,
  onQuickAssign,
  onCheckReadings,
}: {
  summary: EnergySummary;
  check: MeterCheck | null;
  pending: boolean;
  error: string | null;
  onAssign: () => void;
  onQuickAssign: (from: string, to: string, meter: MeterNumber) => void;
  onCheckReadings: () => void;
}) {
  const items: AttentionItem[] = [];
  const unassigned = summary.bill?.so_far.unassigned_kwh ?? 0;
  const suspect = check?.suspect;
  if (unassigned >= 0.005) {
    const window = summary.bill ? unassignedWindow(summary.bill.start, summary.assignments) : null;
    items.push({
      key: "unassigned",
      text: (
        <>
          <span className="font-medium text-foreground">{unitsLabel(unassigned)}</span> in the log aren't on a meter yet
          {window ? ` (${readingMoment(window.from)} – ${readingMoment(window.to)})` : ""}
          {suspect ? ` — Meter ${suspect.meter} rose ${unitsLabel(suspect.used)} this bill, so they're probably Meter ${suspect.meter}'s` : ""}
        </>
      ),
      actions:
        suspect && window
          ? [
              { label: `Assign to Meter ${suspect.meter}`, onClick: () => onQuickAssign(window.from, window.to, suspect.meter) },
              { label: "Other…", onClick: onAssign },
            ]
          : [{ label: "Assign", onClick: onAssign }],
    });
  } else if (suspect) {
    items.push({
      key: "suspect",
      text: `Meter ${suspect.meter} rose ${unitsLabel(suspect.used)} this bill but the log has almost nothing on it — the last-bill reading may be an estimate`,
      actions: [{ label: "Fix readings", onClick: onCheckReadings }],
    });
  }
  const gaps = [
    { label: "this bill", period: summary.bill },
    { label: "last bill", period: summary.previous_bill },
  ]
    .map(({ label, period }) => ({ label, hours: period ? period.elapsed_hours - period.log_hours : 0 }))
    .filter((gap) => gap.hours >= 1);
  if (gaps.length > 0) {
    items.push({
      key: "gaps",
      text: `The inverter's log is missing ${gaps.map((gap) => `${fmtNumber(gap.hours, 1)} h ${gap.label}`).join(" and ")} · real figures a little higher`,
      actions: [],
    });
  }
  if (items.length === 0) return null;

  return (
    <div className="space-y-1.5 border-t px-4 sm:px-5 py-3 text-xs tabular-nums text-muted-foreground">
      <ul className="space-y-1.5" aria-label="Needs attention">
        {items.map((item) => (
          <li key={item.key} className="flex items-start gap-2">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
            <span>
              {item.text}
              {item.actions.map((action) => (
                <span key={action.label}>
                  {" "}
                  <button type="button" disabled={pending} onClick={action.onClick} className={TEXT_BUTTON}>
                    {action.label}
                  </button>
                </span>
              ))}
            </span>
          </li>
        ))}
      </ul>
      <ErrorText error={error} />
    </div>
  );
}

function chartStep(peak: number): number {
  if (peak <= 1) return 0.5;
  if (peak <= 3) return 1;
  if (peak <= 6) return 2;
  return 5;
}

interface ChartRow {
  date: string;
  day: DayUnits;
  partial: boolean;
  month: string | null;
  m1: number;
  m2: number;
  none: number;
}

const SEGMENTS: { key: "none" | "m2" | "m1"; fill: "1" | "2" | "none"; label: string }[] = [
  { key: "none", fill: "none", label: "Not assigned" },
  { key: "m2", fill: "2", label: "Meter 2" },
  { key: "m1", fill: "1", label: "Meter 1" },
];

function DayTick({ x, y, payload, rows, todayKey }: { x?: number | string; y?: number | string; payload?: { value: string }; rows: ChartRow[]; todayKey?: string }) {
  const row = rows.find((item) => item.date === payload?.value);
  if (!row || x == null || y == null) return null;
  const today = row.date === todayKey;
  return (
    <g transform={`translate(${x},${y})`}>
      <text dy={10} textAnchor="middle" fontSize={CHART_FONT} fill={today ? "var(--foreground)" : VIZ.ink} fontWeight={today ? 600 : 400}>
        {parseDay(row.date).getDate()}
      </text>
      {row.month ? (
        <text dy={22} textAnchor="middle" fontSize={CHART_FONT - 1} fill={VIZ.ink}>
          {row.month}
        </text>
      ) : null}
    </g>
  );
}

function RecentColumn({ days, billStart }: { days: DayUnits[]; billStart: string | null }) {
  const peak = Math.max(0.5, ...days.map((day) => day.grid_kwh));
  const step = chartStep(peak);
  const top = Math.ceil(peak / step) * step;
  const ticks = Array.from({ length: Math.round(top / step) + 1 }, (_, index) => index * step);
  const todayKey = days[days.length - 1]?.date;
  const finished = days.filter((day) => day.house_kwh > 0 && day.date !== todayKey);
  const average = finished.length > 0 ? finished.reduce((total, day) => total + day.grid_kwh, 0) / finished.length : null;
  const billDay = billStart?.slice(0, 10) ?? null;
  const billInView = billDay != null && days.some((day) => day.date === billDay);
  const rows: ChartRow[] = days.map((day, index) => {
    const date = parseDay(day.date);
    const hasData = day.house_kwh > 0;
    return {
      date: day.date,
      day,
      partial: day.date === todayKey,
      month: index === 0 || date.getDate() === 1 ? date.toLocaleDateString("en-GB", { month: "short" }) : null,
      m1: hasData ? day.meter1_kwh : 0,
      m2: hasData ? day.meter2_kwh : 0,
      none: hasData ? day.unassigned_kwh : 0,
    };
  });
  const present = [...SEGMENTS].reverse().filter((segment) => rows.some((row) => row[segment.key] >= 0.005));

  return (
    <div className="border-t px-4 sm:px-5 py-4 lg:border-t-0">
      <p className="flex items-baseline justify-between text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>Last 14 days</span>
        {average != null ? <span className="normal-case tracking-normal tabular-nums">avg {unitsLabel(average)} a day</span> : null}
      </p>
      <div className="mt-3 h-48" role="img" aria-label={`Units taken from the grid each day for the last 14 days${average != null ? `, averaging ${unitsLabel(average)} a day` : ""}`}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={rows} margin={{ top: 4, right: 0, bottom: 0, left: 0 }} barCategoryGap={3}>
            <CartesianGrid vertical={false} stroke={VIZ.grid} />
            <XAxis
              dataKey="date"
              interval={0}
              height={40}
              tickLine={false}
              axisLine={false}
              tick={(props) => <DayTick {...props} rows={rows} todayKey={todayKey} />}
            />
            <YAxis
              orientation="right"
              domain={[0, top]}
              ticks={ticks}
              tickFormatter={(value: number) => fmtNumber(value, step < 1 ? 1 : 0)}
              tick={{ fontSize: CHART_FONT, fill: VIZ.ink }}
              tickLine={false}
              axisLine={false}
              width={24}
            />
            {billInView ? <ReferenceLine x={billDay} position="start" stroke={VIZ.contextStrong} strokeDasharray="3 3" /> : null}
            <Tooltip
              cursor={{ fill: VIZ.grid }}
              content={({ active, payload }) => {
                const row = payload?.[0]?.payload as ChartRow | undefined;
                if (!active || !row) return null;
                const { day } = row;
                const date = parseDay(day.date).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
                const parts = SEGMENTS.filter((segment) => row[segment.key] >= 0.005).reverse();
                return (
                  <ChartTooltipBox>
                    <p className="font-medium">
                      {date}
                      {row.partial ? " · so far" : ""}
                      {row.date === billDay ? " · new bill" : ""}
                    </p>
                    {day.house_kwh <= 0 ? (
                      <p className="text-muted-foreground">No data</p>
                    ) : (
                      <>
                        <p className="text-muted-foreground">
                          From the grid <span className="text-foreground">{unitsLabel(day.grid_kwh)}</span>
                        </p>
                        {parts.length > 1
                          ? parts.map((segment) => (
                              <p key={segment.key} className="flex items-center gap-1.5 text-muted-foreground">
                                <span className="size-2 rounded-xs" style={{ background: GRID_FILL[segment.fill] }} aria-hidden />
                                {segment.label} <span className="text-foreground">{fmtUnits(row[segment.key])}</span>
                              </p>
                            ))
                          : null}
                      </>
                    )}
                  </ChartTooltipBox>
                );
              }}
            />
            {SEGMENTS.map((segment) => (
              <Bar key={segment.key} dataKey={segment.key} stackId="grid" fill={GRID_FILL[segment.fill]} isAnimationActive={false}>
                {rows.map((row) => (
                  <Cell key={row.date} fillOpacity={row.partial ? 0.55 : 1} />
                ))}
              </Bar>
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-micro text-muted-foreground">
        {present.map((segment) => (
          <Legend key={segment.key} fill={GRID_FILL[segment.fill]} label={segment.label} />
        ))}
        {billInView ? (
          <span className="inline-flex items-center gap-1.5">
            <span className="h-2.5 border-l border-dashed border-muted-foreground/60" aria-hidden />
            New bill
          </span>
        ) : null}
      </div>
    </div>
  );
}

function Legend({ fill, label }: { fill: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="size-2 rounded-xs" style={{ background: fill }} aria-hidden />
      {label}
    </span>
  );
}

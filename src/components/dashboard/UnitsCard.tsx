import { TriangleAlert } from "lucide-react";
import { Bar, BarChart, CartesianGrid, Cell, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useState, type CSSProperties, type FormEvent } from "react";
import { CHART_FONT, ChartTooltipBox, VIZ } from "@/components/automation/shared";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useEnergyUnits } from "@/hooks/useEnergyUnits";
import { fmtNumber, fmtTime } from "@/lib/format";
import type { BillPeriod, DayUnits, EnergySummary, GridSplit, MeterAssignment, MeterNumber } from "@/lib/types";
import { cn } from "@/lib/utils";

const GRID_FILL: Record<"1" | "2" | "none", string> = {
  "1": "var(--viz-meter-1)",
  "2": "var(--viz-meter-2)",
  none: "var(--viz-unassigned)",
};
const READING_DAYS = Array.from({ length: 28 }, (_, index) => index + 1);

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

export function UnitsCard() {
  const { summary, saveReading, saveStandby, switchMeter, assignRange, removeAssignment } = useEnergyUnits();

  if (!summary) {
    return <section className="h-64 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card" aria-labelledby="units-title">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2 border-b px-4 sm:px-5 py-3">
        <div>
          <h2 id="units-title" className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Electricity units
          </h2>
          <p className="text-micro text-muted-foreground">Taken from the grid · 1 unit = 1 kWh</p>
        </div>
        {summary.configured ? (
          <MeterSwitcher
            active={summary.active_meter}
            since={summary.active_since}
            pending={switchMeter.isPending}
            error={switchMeter.error ? String(switchMeter.error) : null}
            onSwitch={(meter, at) => switchMeter.mutateAsync({ meter, at })}
          />
        ) : null}
      </div>

      {!summary.configured ? (
        <p className="px-4 sm:px-5 py-6 text-sm text-muted-foreground">Add your Solar of Things login in Settings to count units.</p>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-3 lg:divide-x">
          <TodayColumn summary={summary} />
          <BillsColumn summary={summary} />
          <RecentColumn days={summary.recent_days} billStart={summary.bill?.start ?? null} />
        </div>
      )}

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t px-4 sm:px-5 py-3 text-xs text-muted-foreground">
        <ReadingPicker
          day={summary.bill_reading_day}
          time={summary.bill_reading_time}
          pending={saveReading.isPending}
          error={saveReading.error ? String(saveReading.error) : null}
          onSave={(day, time) => saveReading.mutate({ day, time })}
        />
        <StandbyField
          watts={summary.standby_w}
          pending={saveStandby.isPending}
          error={saveStandby.error ? String(saveStandby.error) : null}
          onSave={(watts) => saveStandby.mutate(watts)}
        />
        <StatusLine summary={summary} />
        <AssignPast
          assignments={summary.assignments}
          unassigned={summary.bill?.so_far.unassigned_kwh ?? 0}
          pending={assignRange.isPending || removeAssignment.isPending}
          error={assignRange.error ? String(assignRange.error) : removeAssignment.error ? String(removeAssignment.error) : null}
          onAssign={(from, to, meter) => assignRange.mutateAsync({ from, to, meter })}
          onRemove={(from) => removeAssignment.mutate(from)}
        />
        <p className="basis-full leading-snug">
          Counted from the inverter's 5-minute log: the house load that solar and the battery didn't cover while the grid was on,
          plus the inverter's own draw from the grid, which its load reading doesn't show. Raise or lower the watts until the
          app matches your meter. Loads not wired through the inverter, and grid charging of the battery, aren't counted.
        </p>
      </div>
    </section>
  );
}

function localInputValue(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function AssignPast({
  assignments,
  unassigned,
  pending,
  error,
  onAssign,
  onRemove,
}: {
  assignments: MeterAssignment[];
  unassigned: number;
  pending: boolean;
  error: string | null;
  onAssign: (from: string, to: string, meter: MeterNumber) => Promise<unknown>;
  onRemove: (from: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [meter, setMeter] = useState<MeterNumber>(2);
  const field =
    "h-7 rounded-md border border-border bg-background px-1.5 text-xs pointer-coarse:h-10 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

  const begin = () => {
    setOpen(true);
    setTo(localInputValue(new Date()));
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!from || !to) return;
    try {
      await onAssign(from, to, meter);
      setFrom("");
      setOpen(false);
    } catch {
      return;
    }
  };

  return (
    <div className="basis-full space-y-2">
      {!open ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" variant="outline" size="sm" onClick={begin}>
            Assign past units to a meter
          </Button>
          {unassigned >= 0.005 ? <span className="tabular-nums">{unitsLabel(unassigned)} this bill aren't assigned yet.</span> : null}
        </div>
      ) : (
        <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
          <label className="inline-flex items-center gap-1.5">
            From
            <input type="datetime-local" className={field} value={from} max={to || undefined} onChange={(event) => setFrom(event.target.value)} />
          </label>
          <label className="inline-flex items-center gap-1.5">
            to
            <input type="datetime-local" className={field} value={to} min={from || undefined} onChange={(event) => setTo(event.target.value)} />
          </label>
          <label className="inline-flex items-center gap-1.5">
            was on
            <select className={field} value={meter} onChange={(event) => setMeter(Number(event.target.value) as MeterNumber)}>
              <option value={1}>Meter 1</option>
              <option value={2}>Meter 2</option>
            </select>
          </label>
          <Button type="submit" size="sm" disabled={pending || !from || !to}>
            Assign
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => setOpen(false)}>
            Cancel
          </Button>
        </form>
      )}
      {assignments.length > 0 ? (
        <ul className="space-y-1">
          {assignments.map((assignment) => (
            <li key={assignment.from} className="flex flex-wrap items-center gap-2 tabular-nums">
              <span>
                {readingMoment(assignment.from)} – {readingMoment(assignment.to)}: Meter {assignment.meter}
              </span>
              <button
                type="button"
                disabled={pending}
                onClick={() => onRemove(assignment.from)}
                className="rounded px-1.5 text-foreground underline underline-offset-2 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      ) : null}
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
  const field =
    "h-7 rounded-md border border-border bg-background px-1.5 text-xs pointer-coarse:h-10 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";
  return (
    <div className="inline-flex flex-wrap items-center gap-2">
      <label className="inline-flex items-center gap-2">
        Meters read on the
        <select className={field} value={day} disabled={pending} onChange={(event) => onSave(Number(event.target.value), time)}>
          {READING_DAYS.map((option) => (
            <option key={option} value={option}>
              {ordinal(option)}
            </option>
          ))}
        </select>
      </label>
      <label className="inline-flex items-center gap-2">
        at
        <input
          type="time"
          className={field}
          value={draftTime}
          disabled={pending}
          onChange={(event) => setDraftTime(event.target.value)}
          onBlur={saveTime}
          onKeyDown={(event) => {
            if (event.key === "Enter") saveTime();
          }}
        />
      </label>
      <span>of each month</span>
      {error ? (
        <span role="alert" className="text-destructive">
          {error}
        </span>
      ) : null}
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
    <label className="inline-flex items-center gap-2">
      Inverter's own draw
      <input
        type="number"
        inputMode="decimal"
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
        className="h-7 w-14 rounded-md border border-border bg-background px-1.5 text-xs pointer-coarse:h-10 pointer-coarse:w-16 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      />
      W
      {error ? (
        <span role="alert" className="text-destructive">
          {error}
        </span>
      ) : null}
    </label>
  );
}

function StatusLine({ summary }: { summary: EnergySummary }) {
  if (summary.backfill) {
    return (
      <span className="tabular-nums" role="status">
        Filling in past days… {summary.backfill.done} of {summary.backfill.total}
      </span>
    );
  }
  if (summary.error) {
    return (
      <span className="text-destructive" role="alert">
        {summary.error}
      </span>
    );
  }
  return summary.updated_at ? <span className="tabular-nums">Updated {fmtTime(summary.updated_at)}</span> : null;
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
        className="flex gap-[3px] rounded-md bg-neutral-900 p-[3px] shadow-inner ring-1 ring-black/10 dark:bg-black/70 dark:ring-white/10"
      >
        {digits.split("").map((digit, index) => (
          <Digit key={`whole-${index}`} digit={digit} dim={index < leading} />
        ))}
        {fraction.split("").map((digit, index) => (
          <Digit key={`fraction-${index}`} digit={digit} fraction className={index === 0 ? "ml-1" : undefined} />
        ))}
      </div>
      <span className="pb-1 text-sm font-medium text-muted-foreground">units</span>
    </div>
  );
}

function Digit({ digit, dim = false, fraction = false, className }: { digit: string; dim?: boolean; fraction?: boolean; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid h-10 w-7 place-items-center rounded-[3px] bg-linear-to-b from-white/10 via-transparent to-white/5 text-xl sm:h-11 sm:w-8 sm:text-2xl font-semibold leading-none tabular-nums",
        fraction ? "bg-[var(--viz-accent)] text-white" : dim ? "text-neutral-500" : "text-neutral-100",
        className,
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
      <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Today so far</p>
      {today ? (
        <>
          <MeterRegister kwh={today.grid_kwh} />
          <p className="mt-3 text-sm tabular-nums">
            The house used <span className="font-semibold">{today.house_kwh.toFixed(2)} kWh</span>
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
            Yesterday <span className="font-semibold text-foreground">{unitsLabel(yesterday.grid_kwh)}</span> ·{" "}
            {fmtNumber(Math.max(0, yesterday.house_kwh - yesterday.grid_kwh), 1)} kWh came from solar and the battery
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
    { label: "Grid · Meter 1", kwh: day.meter1_kwh, style: { background: GRID_FILL["1"] } },
    { label: "Grid · Meter 2", kwh: day.meter2_kwh, style: { background: GRID_FILL["2"] } },
    { label: "Grid · not assigned", kwh: day.unassigned_kwh, style: { background: GRID_FILL.none } },
  ].filter((part) => part.kwh >= 0.005);
  const parts: { label: string; kwh: number; style?: CSSProperties }[] = [
    ...(grid.length > 0 ? grid : [{ label: "Grid", kwh: 0, style: { background: GRID_FILL.none } }]),
    { label: "Solar", kwh: day.solar_kwh },
    { label: "Battery", kwh: day.battery_kwh },
  ];
  const sum = parts.reduce((total, part) => total + part.kwh, 0);
  const total = sum > 0 ? sum : 1;
  return (
    <>
      <div className="mt-2 flex h-2 overflow-hidden rounded-full bg-muted" aria-hidden>
        {grid.map((part) => (
          <div key={part.label} style={{ ...part.style, width: `${(part.kwh / total) * 100}%` }} />
        ))}
      </div>
      <dl className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs tabular-nums text-muted-foreground">
        {parts.map((part) => (
          <div key={part.label} className="inline-flex items-center gap-1.5">
            <span className={cn("size-2.5 rounded-xs", !part.style && "bg-muted")} style={part.style} aria-hidden />
            <dt>{part.label}</dt>
            <dd className="font-medium text-foreground">{part.kwh.toFixed(2)} kWh</dd>
          </div>
        ))}
      </dl>
    </>
  );
}


function MeterSwitcher({
  active,
  since,
  pending,
  error,
  onSwitch,
}: {
  active: MeterNumber | null;
  since: string | null;
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

  const sinceDate = since ? new Date(since) : null;
  const sinceText =
    sinceDate == null
      ? "Tap the meter the changeover switch is on now"
      : sinceDate.toDateString() === new Date().toDateString()
        ? `since ${fmtTime(since)}`
        : `since ${sinceDate.toLocaleDateString("en-GB", { day: "numeric", month: "short" })}, ${fmtTime(since)}`;

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
          {([1, 2] as const).map((meter) => (
            <ToggleGroupItem
              key={meter}
              value={String(meter)}
              size="sm"
              className="h-auto gap-1.5 rounded px-2.5 py-1 font-medium"
            >
              <span className="size-2 rounded-full" style={{ background: GRID_FILL[String(meter) as "1" | "2"] }} aria-hidden />
              Meter {meter}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      {target == null ? <p className="text-micro tabular-nums text-muted-foreground">{sinceText}</p> : null}
      {target != null ? (
        <form onSubmit={submit} className="flex flex-wrap items-center gap-2 text-xs sm:justify-end">
          <label className="inline-flex items-center gap-1.5">
            Switched to Meter {target} at
            <input
              type="time"
              value={time}
              max={openedAt}
              onChange={(event) => setTime(event.target.value)}
              className="h-7 rounded-md border border-border bg-background px-1.5 pointer-coarse:h-10 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
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

function BillsColumn({ summary }: { summary: EnergySummary }) {
  return (
    <div className="space-y-4 border-t px-4 sm:px-5 py-4 lg:border-t-0">
      {summary.bill ? <BillBlock title="This bill" period={summary.bill} active={summary.active_meter} /> : null}
      {summary.previous_bill ? (
        <BillBlock title="Last bill" period={summary.previous_bill} active={summary.active_meter} previous />
      ) : null}
    </div>
  );
}

function BillBlock({ title, period, active, previous = false }: { title: string; period: BillPeriod; active: MeterNumber | null; previous?: boolean }) {
  const rows: { key: "1" | "2" | "none"; label: string; kwh: number; projected?: number }[] = [
    { key: "1", label: "Meter 1", kwh: period.so_far.meter1_kwh, projected: period.projected?.meter1_kwh },
    { key: "2", label: "Meter 2", kwh: period.so_far.meter2_kwh, projected: period.projected?.meter2_kwh },
    { key: "none", label: "Not assigned", kwh: period.so_far.unassigned_kwh, projected: period.projected?.unassigned_kwh },
  ];
  const shown = rows.filter((row) => row.kwh >= 0.005 || (row.key !== "none" && active != null && row.key === String(active)));
  const missing = period.days_total - period.days_with_data;
  const single = shown.length <= 1;
  return (
    <div>
      <p className="flex flex-wrap items-baseline justify-between gap-x-2 text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>{title}</span>
        <span className="normal-case tracking-normal tabular-nums">
          {readingMoment(period.start)} – {readingMoment(period.end)}
        </span>
      </p>
      {shown.length === 0 ? (
        <p className="mt-2 text-sm text-muted-foreground">No units recorded for this period yet.</p>
      ) : (
        <ul className="mt-2 space-y-1.5">
          {shown.map((row) => (
            <li key={row.key} className="flex items-baseline gap-2 text-sm tabular-nums">
              <span className="size-2 shrink-0 translate-y-[-1px] rounded-full" style={{ background: GRID_FILL[row.key] }} aria-hidden />
              <span className="text-muted-foreground">{row.label}</span>
              <span className="ml-auto font-semibold">{unitsLabel(row.kwh)}</span>
              {row.projected != null && row.projected - row.kwh >= 0.05 ? (
                <span className="w-24 text-right text-xs text-muted-foreground">~{fmtUnits(row.projected)} by the end</span>
              ) : !previous ? (
                <span className="w-24" aria-hidden />
              ) : null}
            </li>
          ))}
        </ul>
      )}
      <PeriodTotal period={period} previous={previous} missing={missing} single={single} />
      <LogGap period={period} />
    </div>
  );
}

function LogGap({ period }: { period: BillPeriod }) {
  const gap = period.elapsed_hours - period.log_hours;
  if (gap < 1) return null;
  return (
    <p
      className="mt-1.5 inline-flex items-center gap-1.5 text-xs tabular-nums text-muted-foreground"
      title="Units used during those hours aren't counted, so the real figure is a little higher."
    >
      <TriangleAlert className="size-3.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
      {fmtNumber(gap, 1)} h missing from the inverter's log · real figure a little higher
    </p>
  );
}

function PeriodTotal({ period, previous, missing, single }: { period: BillPeriod; previous: boolean; missing: number; single: boolean }) {
  const total: GridSplit = period.so_far;
  const noData = missing > 0 ? `${missing} ${missing === 1 ? "day has" : "days have"} no data.` : "";
  if (single) {
    return noData ? <p className="mt-2 text-xs tabular-nums text-muted-foreground">{noData}</p> : null;
  }
  if (previous) {
    return (
      <p className="mt-2 text-xs tabular-nums text-muted-foreground">
        {unitsLabel(total.grid_kwh)} in all — compare with your bills.{noData ? ` ${noData}` : ""}
      </p>
    );
  }
  return (
    <p className="mt-2 text-xs tabular-nums text-muted-foreground">
      {unitsLabel(total.grid_kwh)} so far
      {period.projected ? ` · on pace for ~${fmtUnits(period.projected.grid_kwh)} by ${dayMonth(period.end)}` : ""}
    </p>
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

  return (
    <div className="border-t px-4 sm:px-5 py-4 lg:border-t-0">
      <p className="flex items-baseline justify-between text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>Last 14 days</span>
        {average != null ? <span className="normal-case tracking-normal tabular-nums">avg {fmtUnits(average)} a day</span> : null}
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
        <Legend fill={GRID_FILL["1"]} label="Meter 1" />
        <Legend fill={GRID_FILL["2"]} label="Meter 2" />
        <Legend fill={GRID_FILL.none} label="Not assigned" />
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

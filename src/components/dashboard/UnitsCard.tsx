import { useState, type CSSProperties, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/switch";
import { useEnergyUnits } from "@/hooks/useEnergyUnits";
import { fmtNumber, fmtTime } from "@/lib/format";
import type { BillPeriod, DayUnits, EnergySummary, GridSplit, MeterNumber } from "@/lib/types";
import { cn } from "@/lib/utils";

const GRID_FILL: Record<"1" | "2" | "none", string> = {
  "1": "var(--viz-accent)",
  "2": "color-mix(in oklch, var(--viz-accent) 42%, transparent)",
  none: "var(--viz-context-strong)",
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
  const { summary, saveReading, saveStandby, switchMeter } = useEnergyUnits();

  if (!summary) {
    return <section className="h-64 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
  }

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card" aria-labelledby="units-title">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2 border-b px-5 py-3">
        <div>
          <h2 id="units-title" className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Electricity units
          </h2>
          <p className="text-[11px] text-muted-foreground">Taken from the grid · 1 unit = 1 kWh</p>
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
        <p className="px-5 py-6 text-sm text-muted-foreground">Add your Solar of Things login in Settings to count units.</p>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-3 lg:divide-x">
          <TodayColumn summary={summary} />
          <BillsColumn summary={summary} />
          <RecentColumn days={summary.recent_days} />
        </div>
      )}

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t px-5 py-3 text-xs text-muted-foreground">
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
        <p className="basis-full leading-snug">
          Counted from the inverter's 5-minute log: the house load that solar and the battery didn't cover while the grid was on,
          plus the inverter's own draw from the grid, which its load reading doesn't show. Raise or lower the watts until the
          app matches your meter. Loads not wired through the inverter, and grid charging of the battery, aren't counted.
        </p>
      </div>
    </section>
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
    "h-7 rounded-md border border-border bg-background px-1.5 text-xs tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";
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
        className="h-7 w-14 rounded-md border border-border bg-background px-1.5 text-xs tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
        "grid h-11 w-8 place-items-center rounded-[3px] bg-linear-to-b from-white/10 via-transparent to-white/5 text-2xl font-semibold leading-none tabular-nums",
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
    <div className="px-5 py-4">
      <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Today so far</p>
      {today ? (
        <>
          <MeterRegister kwh={today.grid_kwh} />
          <p className="mt-3 text-sm tabular-nums">
            The house used <span className="font-semibold">{fmtNumber(today.house_kwh, 2)} kWh</span>
          </p>
          <ShareBar day={today} />
        </>
      ) : summary.error ? (
        <p className="mt-2 text-sm text-muted-foreground">Today's numbers show up once the inverter cloud answers.</p>
      ) : (
        <div className="mt-2 space-y-2">
          <Skeleton className="h-12 w-52" />
          <Skeleton className="h-3 w-40" />
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
  const parts: { label: string; kwh: number; className?: string; style?: CSSProperties }[] = [
    { label: "Solar", kwh: day.solar_kwh, className: "bg-amber-500" },
    { label: "Battery", kwh: day.battery_kwh, className: "bg-sky-500" },
    { label: "Grid", kwh: day.grid_kwh, style: { background: GRID_FILL["1"] } },
  ];
  const sum = parts.reduce((total, part) => total + part.kwh, 0);
  const total = sum > 0 ? sum : 1;
  return (
    <>
      <div className="mt-2 flex h-2 overflow-hidden rounded-full bg-muted" aria-hidden>
        {parts.map((part) =>
          part.kwh > 0 ? (
            <div key={part.label} className={part.className} style={{ ...part.style, width: `${(part.kwh / total) * 100}%` }} />
          ) : null,
        )}
      </div>
      <dl className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs tabular-nums text-muted-foreground">
        {parts.map((part) => (
          <div key={part.label} className="inline-flex items-center gap-1.5">
            <span className={cn("size-2 rounded-full", part.className)} style={part.style} aria-hidden />
            <dt>{part.label}</dt>
            <dd className="font-medium text-foreground">{fmtNumber(part.kwh, 2)} kWh</dd>
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
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-2 text-xs">
        <span className="text-muted-foreground">Grid from</span>
        <div role="radiogroup" aria-label="Meter the changeover switch is on" className="inline-flex rounded-md border border-border p-0.5">
          {([1, 2] as const).map((meter) => (
            <button
              key={meter}
              type="button"
              role="radio"
              aria-checked={active === meter}
              disabled={pending}
              onClick={() => begin(meter)}
              className={cn(
                "inline-flex items-center gap-1.5 rounded px-2.5 py-1 font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                active === meter ? "bg-foreground text-background" : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <span className="size-2 rounded-full" style={{ background: GRID_FILL[String(meter) as "1" | "2"] }} aria-hidden />
              Meter {meter}
            </button>
          ))}
        </div>
      </div>
      {target == null ? <p className="text-[11px] tabular-nums text-muted-foreground">{sinceText}</p> : null}
      {target != null ? (
        <form onSubmit={submit} className="flex flex-wrap items-center justify-end gap-2 text-xs">
          <label className="inline-flex items-center gap-1.5">
            Switched to Meter {target} at
            <input
              type="time"
              value={time}
              max={openedAt}
              onChange={(event) => setTime(event.target.value)}
              className="h-7 rounded-md border border-border bg-background px-1.5 tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
        <p role="alert" className="text-[11px] text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function BillsColumn({ summary }: { summary: EnergySummary }) {
  return (
    <div className="space-y-4 border-t px-5 py-4 lg:border-t-0">
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
  return (
    <div>
      <p className="flex flex-wrap items-baseline justify-between gap-x-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
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
      <PeriodTotal period={period} previous={previous} missing={missing} />
      <LogGap period={period} />
    </div>
  );
}

function LogGap({ period }: { period: BillPeriod }) {
  const gap = period.elapsed_hours - period.log_hours;
  if (gap < 1) return null;
  return (
    <p className="mt-1 text-xs tabular-nums text-amber-700 dark:text-amber-400">
      The inverter's log is missing {fmtNumber(gap, 1)} h of this period, so the real figure is a little higher.
    </p>
  );
}

function PeriodTotal({ period, previous, missing }: { period: BillPeriod; previous: boolean; missing: number }) {
  const total: GridSplit = period.so_far;
  if (previous) {
    return (
      <p className="mt-2 text-xs tabular-nums text-muted-foreground">
        {unitsLabel(total.grid_kwh)} in all — compare with your bills.
        {missing > 0 ? ` ${missing} ${missing === 1 ? "day has" : "days have"} no data.` : ""}
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

function RecentColumn({ days }: { days: DayUnits[] }) {
  const max = Math.max(0.5, ...days.map((day) => day.grid_kwh));
  const todayKey = days[days.length - 1]?.date;
  return (
    <div className="border-t px-5 py-4 lg:border-t-0">
      <p className="flex items-baseline justify-between text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        <span>Last 14 days</span>
        <span className="normal-case tracking-normal tabular-nums">tallest {unitsLabel(max)}</span>
      </p>
      <div className="mt-3 flex h-32 items-stretch gap-1">
        {days.map((day) => {
          const hasData = day.house_kwh > 0;
          const label = hasData
            ? `${parseDay(day.date).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" })}: ${unitsLabel(day.grid_kwh)}`
            : `${dayMonth(day.date)}: no data`;
          const segments: { key: "1" | "2" | "none"; kwh: number }[] = [
            { key: "none", kwh: day.unassigned_kwh },
            { key: "2", kwh: day.meter2_kwh },
            { key: "1", kwh: day.meter1_kwh },
          ];
          return (
            <div key={day.date} role="img" aria-label={label} title={label} className="flex min-w-0 flex-1 flex-col items-center gap-1">
              <div className="flex w-full flex-1 flex-col justify-end">
                {hasData ? (
                  segments.map((segment) =>
                    segment.kwh > 0 ? (
                      <div
                        key={segment.key}
                        className="w-full first:rounded-t-[2px]"
                        style={{ height: `${(segment.kwh / max) * 100}%`, background: GRID_FILL[segment.key] }}
                      />
                    ) : null,
                  )
                ) : (
                  <div className="w-full border-t border-dashed border-muted-foreground/40" />
                )}
              </div>
              <span className={cn("text-[10px] tabular-nums text-muted-foreground", day.date === todayKey && "font-semibold text-foreground")}>
                {parseDay(day.date).getDate()}
              </span>
            </div>
          );
        })}
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
        <Legend fill={GRID_FILL["1"]} label="Meter 1" />
        <Legend fill={GRID_FILL["2"]} label="Meter 2" />
        <Legend fill={GRID_FILL.none} label="Not assigned" />
      </div>
    </div>
  );
}

function Legend({ fill, label }: { fill: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="size-2 rounded-[2px]" style={{ background: fill }} aria-hidden />
      {label}
    </span>
  );
}

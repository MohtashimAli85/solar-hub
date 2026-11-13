import { clock } from "@/components/automation/shared";
import { Button } from "@/components/ui/button";
import type { LoadEdit, LoadKind, LoadRow, LoadsStatus, WaterStatus } from "@/lib/types";
import { Bike, Check, CookingPot, Droplets, Flame, MessageCircleQuestion, type LucideIcon } from "lucide-react";

interface Action {
  label: string;
  primary?: boolean;
  run: () => void;
}

interface Item {
  key: string;
  icon: LucideIcon;
  text: string;
  actions: Action[];
}

const ICONS: Record<LoadKind, LucideIcon> = { water: Droplets, geyser: Flame, hotplate: CookingPot, ev: Bike };
const NAMES: Record<LoadKind, string> = { water: "Water tank", geyser: "Geyser", hotplate: "Hotplate", ev: "Scooter" };

function edit(load: LoadKind, state: LoadEdit["state"], watts: number | null = null): LoadEdit {
  return { load, state, watts, kwh: null, soc: null };
}

/** What a "Turn on" or "Switch off/down" row asks the family to do, with the buttons that confirm it. */
function rowItem(kind: LoadKind, row: LoadRow, onEdit: (edit: LoadEdit) => void): Item | null {
  const text = row.note ?? NAMES[kind];
  if (row.phase === "ready") {
    if (kind === "hotplate") return null;
    const actions: Action[] =
      kind === "ev"
        ? [{ label: "Plugged in", primary: true, run: () => onEdit(edit("ev", "on")) }]
        : kind === "water"
          ? [
              { label: "It's on", primary: true, run: () => onEdit(edit("water", "on")) },
              { label: "Already filled", run: () => onEdit(edit("water", "done")) },
            ]
          : [{ label: "It's on", primary: true, run: () => onEdit(edit(kind, "on", row.watts)) }];
    return { key: `${kind}-ready`, icon: ICONS[kind], text, actions };
  }
  if (row.phase === "stop") {
    const lower = row.watts != null && (kind === "geyser" || kind === "hotplate");
    const done = lower ? edit(kind, "on", row.watts) : edit(kind, kind === "hotplate" ? "done" : "off");
    return { key: `${kind}-stop`, icon: ICONS[kind], text, actions: [{ label: "Done", primary: true, run: () => onEdit(done) }] };
  }
  return null;
}

/**
 * Only what needs the family's hands: switch something on or off, plug in,
 * answer a question, or a charge that must happen today. Nothing to do,
 * nothing shown.
 */
export function NowStrip({
  loads,
  water,
  pending,
  onEdit,
  onAnswer,
}: {
  loads: LoadsStatus | undefined;
  water: WaterStatus | undefined;
  pending: boolean;
  onEdit: (edit: LoadEdit) => void;
  onAnswer: (yes: boolean) => void;
}) {
  if (!loads) return null;
  const items: Item[] = [];
  if (loads.question) {
    items.push({
      key: "question",
      icon: MessageCircleQuestion,
      text: loads.question.text,
      actions: [
        { label: "Yes", primary: true, run: () => onAnswer(true) },
        { label: "No", run: () => onAnswer(false) },
      ],
    });
  }
  if (loads.tank_question) {
    items.push({
      key: "tank",
      icon: Droplets,
      text: `The motor stopped at ${clock(loads.tank_question)}. Is the tank filled?`,
      actions: [
        { label: "Yes", primary: true, run: () => onEdit(edit("water", "done")) },
        { label: "No", run: () => onEdit(edit("water", "off")) },
      ],
    });
  }
  const waterRow: LoadRow | null =
    loads.ai.mode === "ai"
      ? loads.water
      : water?.phase === "ready"
        ? { ...loads.water, phase: "ready", note: water.note ?? "Good solar: turn on the motor." }
        : null;
  const rows: [LoadKind, LoadRow | null][] = [
    ["water", waterRow],
    ["geyser", loads.geyser],
    ["hotplate", loads.hotplate],
    ["ev", loads.ev],
  ];
  for (const [kind, row] of rows) {
    const item = row ? rowItem(kind, row, onEdit) : null;
    if (item) items.push(item);
  }
  const geyser = loads.geyser;
  if (loads.geyser_required && geyser.running_w == null && geyser.phase === "waiting") {
    const when = geyser.from ? ` Best from ${clock(geyser.from)}${geyser.until ? ` to ${clock(geyser.until)}` : ""}.` : "";
    items.push({
      key: "geyser-today",
      icon: Flame,
      text: `Heat the geyser today — ${loads.geyser_required}.${when}`,
      actions: [],
    });
  }
  const ev = loads.ev;
  if (ev.charge_today === true && ev.running_w == null && ev.phase === "waiting") {
    const when = ev.from ? ` Best from ${clock(ev.from)}${ev.until ? ` to ${clock(ev.until)}` : ""}.` : "";
    items.push({
      key: "ev-today",
      icon: Bike,
      text: `Charge the scooter today${ev.charge_rule ? ` — ${ev.charge_rule}` : ""}.${when}`,
      actions: [{ label: "Plugged in", run: () => onEdit(edit("ev", "on")) }],
    });
  }
  if (items.length === 0) return null;

  const next = rows
    .filter(([kind, row]) => row?.phase === "waiting" && row.from != null && !items.some((item) => item.key === `${kind}-today`))
    .sort(([, a], [, b]) => (a?.from ?? "").localeCompare(b?.from ?? ""))[0];

  return (
    <section aria-label="Do this now" className="rounded-xl border border-border bg-card px-4 py-3 sm:px-5">
      <ul className="divide-y divide-border">
        {items.map((item) => (
          <li key={item.key} className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-2 first:pt-0 last:pb-0">
            <p className="flex min-w-0 flex-1 items-center gap-2.5 text-sm font-medium">
              <item.icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
              <span>{item.text}</span>
            </p>
            <div className="flex shrink-0 gap-2">
              {item.actions.map((action) => (
                <Button key={action.label} size="sm" variant={action.primary ? "default" : "ghost"} onClick={action.run} disabled={pending}>
                  {action.primary ? <Check /> : null} {action.label}
                </Button>
              ))}
            </div>
          </li>
        ))}
      </ul>
      {next?.[1]?.from ? (
        <p className="mt-2 text-xs text-muted-foreground">
          Next: {NAMES[next[0]].toLowerCase()} at {clock(next[1].from)}
          {next[1].watts != null ? ` (${next[1].watts.toFixed(0)} W)` : ""}
        </p>
      ) : null}
    </section>
  );
}

import { useState } from "react";
import { clock } from "@/components/automation/shared";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useAutomation } from "@/hooks/useAutomation";
import { useToastPosition, type ToastPosition } from "@/hooks/useToastPosition";
import { cn } from "@/lib/utils";
import type { LoadEdit, LoadKind, LoadRow, LoadsStatus, WaterStatus } from "@/lib/types";
import { Bike, Check, CookingPot, Droplets, Flame, MessageCircleQuestion, X, type LucideIcon } from "lucide-react";

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

const PLACES: Record<ToastPosition, string> = {
  "top-right": "top-[calc(3.5rem+env(safe-area-inset-top))] md:top-4",
  "bottom-right": "bottom-[calc(4.5rem+env(safe-area-inset-bottom))] flex-col-reverse md:bottom-4",
};

/** The "do this now" toasts, on every page, in the corner picked in Settings. */
export function NowToasts() {
  const automation = useAutomation();
  const { position } = useToastPosition();
  return (
    <NowStrip
      loads={automation.status?.loads}
      water={automation.status?.water}
      pending={automation.loadEdit.isPending || automation.questionAnswer.isPending}
      position={position}
      onEdit={(edit) => automation.loadEdit.mutate(edit)}
      onAnswer={(yes) => automation.questionAnswer.mutate(yes)}
    />
  );
}

/**
 * Only what needs the family's hands: switch something on or off, plug in,
 * answer a question, or a charge that must happen today. Nothing to do,
 * nothing shown. A closed toast comes back when what it says changes.
 */
export function NowStrip({
  loads,
  water,
  pending,
  position,
  onEdit,
  onAnswer,
}: {
  loads: LoadsStatus | undefined;
  water: WaterStatus | undefined;
  pending: boolean;
  position: ToastPosition;
  onEdit: (edit: LoadEdit) => void;
  onAnswer: (yes: boolean) => void;
}) {
  const [closed, setClosed] = useState<string[]>([]);
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
      : water?.phase === "ready" && loads.water.running_w == null && loads.water.phase !== "done"
        ? { ...loads.water, phase: "ready", note: water.note ?? "Good solar: turn on the motor." }
        : null;
  const rows: [LoadKind, LoadRow | null][] = [
    ["water", waterRow],
    ["hotplate", loads.hotplate],
    ["ev", loads.ev],
  ];
  for (const [kind, row] of rows) {
    const item = row ? rowItem(kind, row, onEdit) : null;
    if (item) items.push(item);
  }
  const ev = loads.ev;
  if (ev.charge_today === true && ev.running_w == null && ev.phase === "waiting") {
    const why = ev.charge_rule?.replace(/^Charge today: /, "").replace(/\.$/, "");
    const when = ev.from ? ` Best from ${clock(ev.from)}${ev.until ? ` to ${clock(ev.until)}` : ""}.` : "";
    items.push({
      key: "ev-today",
      icon: Bike,
      text: `Charge the scooter today${why ? ` — ${why}` : ""}.${when}`,
      actions: [{ label: "Plugged in", run: () => onEdit(edit("ev", "on")) }],
    });
  }
  const id = (item: Item) => `${item.key}:${item.text}`;
  const open = items.filter((item) => !closed.includes(id(item)));
  if (open.length === 0) return null;

  return (
    <section
      aria-label="Do this now"
      className={cn("pointer-events-none fixed inset-x-4 z-50 flex flex-col gap-2 md:left-auto md:right-4 md:w-96", PLACES[position])}
    >
      {open.map((item) => (
        <Card key={item.key} role="status" className="pointer-events-auto p-3 shadow-lg">
          <div className="flex items-start gap-2.5">
            <item.icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
            <p className="min-w-0 flex-1 text-sm font-medium">{item.text}</p>
            <Button
              size="icon"
              variant="ghost"
              className="-my-1.5 -mr-1.5 size-7 shrink-0"
              aria-label="Close"
              onClick={() => setClosed((current) => [...current, id(item)])}
            >
              <X />
            </Button>
          </div>
          {item.actions.length > 0 ? (
            <div className="mt-2 flex justify-end gap-2">
              {item.actions.map((action) => (
                <Button key={action.label} size="sm" variant={action.primary ? "default" : "ghost"} onClick={action.run} disabled={pending}>
                  {action.primary ? <Check /> : null} {action.label}
                </Button>
              ))}
            </div>
          ) : null}
        </Card>
      ))}
    </section>
  );
}

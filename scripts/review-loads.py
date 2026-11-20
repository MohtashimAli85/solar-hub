#!/usr/bin/env python3
"""Read a day's loads log and show what the app saw, believed and did, with the moments worth a closer look.

Usage: scripts/review-loads.py [YYYY-MM-DD] [--all]
"""
import json
import sys
from collections import Counter
from datetime import date, datetime
from pathlib import Path

LOG_DIR = Path.home() / "Documents" / "Solar Hub" / "loads"
OVERLOAD_W = 3300
DRAIN_W = 300
LOWEST_GEYSER_W = 800


def clock(at):
    return datetime.fromisoformat(at).strftime("%H:%M")


def watts(value):
    return "-" if value is None else f"{value:.0f}"


def load(day):
    path = LOG_DIR / f"{day}.jsonl"
    if not path.exists():
        sys.exit(f"No loads log for {day} at {path}")
    lines = []
    for raw in path.read_text().splitlines():
        try:
            lines.append(json.loads(raw))
        except json.JSONDecodeError:
            pass
    return lines


def flags(step):
    found = []
    geyser = step["rows"]["geyser"]
    running = step.get("running", {})
    load_w = step.get("load_w") or 0
    if load_w > OVERLOAD_W:
        found.append(f"load {load_w:.0f} W above {OVERLOAD_W}")
    battery = step.get("battery_w")
    if battery is not None and battery < -DRAIN_W and "Geyser" in running:
        found.append(f"battery draining {-battery:.0f} W with the geyser on")
    command = step.get("command")
    if command and step.get("hotplate_maybe_on") and "On(" in command["intent"] and "On(800" not in command["intent"]:
        found.append("geyser above 800 W while the hotplate may be on")
    spare = step.get("spare_w")
    if (
        step.get("full")
        and spare is not None
        and spare >= LOWEST_GEYSER_W
        and "Geyser" not in running
        and geyser["phase"] not in ("done", "off")
    ):
        found.append(f"{spare:.0f} W sun spare, geyser {geyser['phase']}")
    if step.get("unknown_w", 0) > 400:
        found.append(f"{step['unknown_w']:.0f} W the app can't place")
    return found


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    show_all = "--all" in sys.argv
    day = args[0] if args else date.today().isoformat()
    lines = load(day)
    steps = [l for l in lines if "rows" in l]
    presses = [l for l in lines if "pressed" in l]
    ai = [l for l in lines if "ai_attempt" in l]

    print(f"Loads log {day}: {len(steps)} steps, {len(presses)} presses, {len(ai)} AI answers\n")
    print(f"{'time':5}  {'load':>5} {'batt':>6} {'soc':>4} {'spare':>5} {'unk':>5} {'room':>5}  {'running':24} {'geyser':28} what happened")
    wasted_steps = 0
    for step in steps:
        found = flags(step)
        if any("sun spare" in f for f in found):
            wasted_steps += 1
        events = []
        if step.get("command"):
            events.append(f"PRESS {'+'.join(step['command']['buttons'])} {step['command']['intent']}")
        events += [f"ASK {a}" for a in step.get("asks", [])]
        if step.get("answer"):
            events.append(f"ANSWER {step['answer']}")
        events += [f"EDIT {e}" for e in step.get("edits", [])]
        if step.get("request"):
            events.append(f"REQUEST {step['request']}")
        events += [f"CHANGE {c}" for c in step.get("changes", [])]
        if not (show_all or events or found or step.get("fresh")):
            continue
        running = ", ".join(f"{k} {v:.0f}" for k, v in step.get("running", {}).items()) or "-"
        if step.get("hotplate", {}).get("running"):
            running += f", Hotplate~{watts(step['hotplate'].get('share_w'))}"
        geyser = step["rows"]["geyser"]
        g = f"{geyser['phase']} {watts(geyser.get('watts'))}"
        print(
            f"{clock(step['at'])}  {watts(step.get('load_w')):>5} {watts(step.get('battery_w')):>6} {watts(step.get('soc')):>4} "
            f"{watts(step.get('spare_w')):>5} {watts(step.get('unknown_w')):>5} {watts(step.get('room_w')):>5}  {running:24.24} {g:28.28} "
            + " | ".join(events + [f"! {f}" for f in found])
        )
        if events and geyser.get("note"):
            print(f"{'':7}geyser note: {geyser['note']}")

    print("\nPresses:")
    for p in presses:
        print(f"  {clock(p['at'])} {'+'.join(p['pressed'])} {'ok' if p['ok'] else 'FAILED'}")

    failed = [a for a in ai if a.get("problems")]
    print(f"\nAI: {len(ai) - len(failed)} passed, {len(failed)} failed the checks")
    problems = Counter()
    for a in failed:
        for p in a["problems"]:
            problems[p.split(".")[0][:90]] += 1
    for text, count in problems.most_common(6):
        print(f"  {count:3}x {text}")

    last = steps[-1] if steps else None
    if last:
        print(f"\nGeyser delivered today: {last.get('geyser_delivered_kwh')} kWh")
    print(f"Steps with spare sun while the geyser sat off: {wasted_steps}")


if __name__ == "__main__":
    main()

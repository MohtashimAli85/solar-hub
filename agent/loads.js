import { runAgent, isMainModule } from "./lib.js";

const LOAD = { type: "string", enum: ["water", "geyser", "hotplate", "scooter"] };

export const LOADS_SCHEMA = {
  type: "object",
  properties: {
    running: {
      type: "array",
      items: {
        type: "object",
        properties: { load: LOAD, watts: { type: "number" }, since: { type: "string" } },
        required: ["load", "watts", "since"],
      },
    },
    sure: { type: "boolean" },
    question: { type: "string" },
    plan: {
      type: "array",
      items: {
        type: "object",
        properties: { load: LOAD, start: { type: "string" }, end: { type: "string" }, watts: { type: "number" } },
        required: ["load", "start", "end", "watts"],
      },
    },
    scooter_first: { type: "boolean" },
    notes: {
      type: "object",
      properties: {
        water: { type: "string" },
        geyser: { type: "string" },
        hotplate: { type: "string" },
        scooter: { type: "string" },
      },
      required: ["water", "geyser", "hotplate", "scooter"],
    },
    recheck_minutes: { type: "number" },
    reason: { type: "string" },
  },
  required: ["running", "sure", "question", "plan", "scooter_first", "notes", "recheck_minutes", "reason"],
};

const n = (value, unit = "", digits = 0) =>
  value === null || value === undefined ? "unknown" : `${Number(value).toFixed(digits)}${unit}`;

function readings(rows) {
  return rows
    .map((r) => `  ${r.time}  load ${String(n(r.load_w, " W")).padStart(7)}  solar ${String(n(r.pv_w, " W")).padStart(7)}  battery ${n(r.soc, "%")}`)
    .join("\n");
}

function hours(rows) {
  if (!rows || rows.length === 0) return "  (no more sun today)";
  return rows
    .map(
      (r) =>
        `  ${r.time}  panels ${String(n(r.sun_w, " W")).padStart(7)}  house ${String(n(r.house_w, " W")).padStart(6)}  spare for the loads ${n(Math.max(0, r.sun_w - r.house_w), " W")}`,
    )
    .join("\n");
}

function days(rows) {
  if (!rows || rows.length === 0) return "- Forecast unavailable.";
  return rows
    .map(
      (d) =>
        `- ${d.relative} (${d.date})${d.cooking_day ? ", a cooking day" : ""}: ${n(d.cloud_pct, "%")} cloud, ${n(d.rain_prob_pct, "%")} rain chance → about ${n(d.expected_pv_kwh, " kWh", 1)} of solar, ${n(d.expected_spare_kwh, " kWh", 1)} left for the scooter and geyser`,
    )
    .join("\n");
}

function room(input) {
  if (input.load_w == null || input.house_usual_w == null) return "";
  const most = input.house_usual_w + Math.max(input.house_usual_w * 0.5, 400);
  const low = Math.max(0, Math.round(input.load_w - most));
  const high = Math.round(input.load_w + 150);
  return ` So the loads you list as running must add up to between ${low} and ${high} W.`;
}

export function buildPrompt(input) {
  const believed =
    input.believed.length === 0 ? "nothing" : input.believed.map((b) => `${b.load} at ${n(b.watts, " W")} since ${b.since}`).join(", ");
  const held = input.pv_held_back
    ? " The battery is full or charging at its limit, so the inverter holds the panels back: live solar shows only what is used, not what the panels could give — trust the forecast below for what's spare."
    : "";
  const sky =
    input.sky_vs_forecast_pct != null && Math.abs(input.sky_vs_forecast_pct - 100) >= 15
      ? ` For the last half hour the panels gave ${n(input.sky_vs_forecast_pct, "%")} of what the forecast expected, so the sky is ${input.sky_vs_forecast_pct < 100 ? "darker" : "brighter"} than forecast. The hours below already assume it stays like this; don't plan on the forecast's original numbers, and check again soon in case it changes.`
      : "";
  const answers = input.answers.length ? `\nWhat the family told the app today (true; the latest line for a load wins, until the readings show it changed since):\n${input.answers.map((a) => `- ${a}`).join("\n")}\n` : "";
  const previous = input.previous ? `\nYour last decision (${input.previous})\n` : "";
  const problems = input.problems.length
    ? `\nYOUR PREVIOUS ANSWER FAILED THE APP'S CHECKS. Fix every one of these:\n${input.problems.map((p) => `- ${p}`).join("\n")}\n`
    : "";
  return `Solar loads decision. It is ${input.now}, ${input.time}; sunset ${input.sunset}. Times are 24-hour HH:MM.

House battery: ${n(input.soc, "%")} of ${n(input.battery_kwh, " kWh", 1)}${input.battery_w != null ? `, ${n(input.battery_w, " W")} going in` : ""}. With no flexible loads the sun would fill it ${input.full_by_sun_alone ? `by ${input.full_by_sun_alone}` : "not at all today"} and leave it at ${n(input.sunset_soc_sun_alone, "%")} at sunset. The family's rules: the battery is back to 100% before the sun fades (your plan may not end the day lower than the sun alone would), and in the day it never goes below ${n(input.day_floor_soc, "%")}. The loads only get sun beyond that. So in the day the battery can lend at most about ${n(input.battery_lend_kwh, " kWh", 1)} to the loads, and only if spare sun later puts it back: keep each hour's runs within that hour's spare sun, borrowing little.

Some loads are marked "Required today" below. Those run today whatever the sun: give each its full run even when the spare sun can't carry it, placed in the sunniest hours so the grid has to add as little as possible. While the battery can't lend it and still be full by sunset, the app switches to Solar mode and the grid tops the load up; the battery isn't drained for it. Loads that aren't required still get only spare sun; on a dark day the hotplate waits and they cook on gas.

Now: the house uses ${n(input.load_w, " W")} and the panels give ${n(input.pv_w, " W")}. Without the loads below, the house usually uses about ${n(input.house_usual_w, " W")} at this hour.${room(input)}${held}${sky}
Readings, last 90 minutes (oldest first):
${readings(input.recent)}
What the app believes is on: ${believed}.

The loads, all switched by hand when the app says so:
- water: ${input.water}
- geyser: ${input.geyser}
- hotplate: ${input.hotplate}
- scooter: ${input.scooter}
Order: ${input.priority} The scooter follows the family's rule in its line above: when it says charge today, give it a run (first if the sun is short); when it says no charge is needed, leave it out.

The rest of today (forecast, corrected by the latest readings):
${hours(input.hours)}

Coming days:
${days(input.next_days)}
${answers}${previous}${problems}
Answer:
- running: every load on right now, its watts and since when. The house load must equal the usual house plus what you list (give or take ~150 W). Read the jumps: +1200 W that stayed is the geyser at 1200 W or the hotplate; two loads can change between readings (the motor going off while the geyser comes on shows as about +200 W), and a load already on before the first reading has no jump at all. Don't drop a load the app believes is on unless the readings show it went off, but the app's beliefs can be wrong: if they add up to more than the house is using, leave out what can't be on. Load above the usual house that the geyser, motor and scooter don't explain (a step of about 800–2200 W) is the hotplate: list it with its watts, even on a day that isn't a cooking day.
- sure: false only when two explanations fit about equally (the geyser at 1200 W or the hotplate at 1200 W). Then question: one short yes/no question naming your best guess, e.g. "Is the geyser on at 1200 W?", and list that guess in running. Otherwise sure: true and question "".
- plan: today's runs, one per load at most: {load, start, end, watts}. A load that's on and should stay on starts now (${input.time}); a load on now but left out of the plan means "switch it off now". Leave out loads that shouldn't run. The geyser runs at exactly 800, 1200 or 2000 W; the hotplate one level for the whole session; the scooter and the motor at their own watts. Keep each load inside its hours and the battery rules. When the battery is full and the panels are held back, start loads now rather than later.
- scooter_first: true only if the scooter goes ahead of the others today.
- notes: one short line per load (at most about 60 characters) for the family's dashboard saying what to do now or next, with 12-hour times, e.g. "Geyser on at 1200 W until 1:30 PM." or "Not a cooking day." Use "" for a load that's off.
- recheck_minutes: 10–30, sooner while things are changing.
- reason: one or two sentences on what decided the plan.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: LOADS_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

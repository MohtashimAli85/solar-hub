import { runAgent, isMainModule } from "./lib.js";

export const NIGHT_SCHEMA = {
  type: "object",
  properties: {
    mode: { type: "string", enum: ["sbg", "solar"] },
    reserve_soc: { type: "number" },
    recheck_minutes: { type: "number" },
    confidence: { type: "number" },
    reason: { type: "string" },
    smart_load_on_at: { type: "string" },
  },
  required: ["mode", "reserve_soc", "recheck_minutes", "confidence", "reason", "smart_load_on_at"],
};

const n = (value, unit = "", digits = 0) =>
  value === null || value === undefined ? "unknown" : `${Number(value).toFixed(digits)}${unit}`;

function lines(rows, format, empty) {
  return rows && rows.length ? rows.map(format).join("\n") : empty;
}

function weatherSection(weather) {
  if (!weather) {
    return "Forecast unavailable right now — assume tomorrow's sun is uncertain and keep a larger reserve.";
  }
  const next = weather.next_day;
  const nextLine = next
    ? `Next solar day (${next.date}): ${n(next.radiation_kwh_m2, " kWh/m²", 1)} radiation, ${n(next.sunshine_h, " h", 1)} sunshine, ${n(next.cloud_pct, "%")} daytime cloud, ${n(next.rain_prob_pct, "%")} rain chance.`
    : "Next solar day: no forecast.";
  const recent = lines(
    weather.recent_days,
    (d) =>
      `- ${d.date}: ${n(d.radiation_kwh_m2, " kWh/m²", 1)} → battery ${d.full_at ? `full by ${d.full_at}` : d.max_soc != null ? `peaked at ${n(d.max_soc, "%")}` : "not recorded"}`,
    "- (no recent days recorded yet)",
  );
  return `${nextLine}
Last 7 days average radiation: ${n(weather.recent_avg_radiation_kwh_m2, " kWh/m²", 1)}.
How recent days actually went (radiation → how the battery charged):
${recent}
${weather.expected_pv_kwh_next_day != null ? `Expected PV energy next solar day from the array size: ${n(weather.expected_pv_kwh_next_day, " kWh", 1)}.` : ""}
Tonight's forecast low: ${n(weather.tonight_min_temp_c, "°C", 1)} vs ${n(weather.recent_nights_min_temp_c, "°C", 1)} average over the last week's nights.`;
}

function smartLoadSection(smart) {
  if (!smart) return "";
  const state = smart.currently_on == null ? "unknown" : smart.currently_on ? "on" : "off";
  if (!smart.choose_on_time) {
    return `Smart load (a switchable circuit — typically cooling like fans/AC) is currently ${state}. It is ${smart.season}, so it comes on at ${smart.earliest} automatically — just return "${smart.earliest}" for smart_load_on_at.`;
  }
  return `Smart load (a switchable circuit — typically cooling like fans/AC) is currently ${state}. It is ${smart.season}, so you choose when it turns on tonight, between ${smart.earliest} and ${smart.latest}. Warmer nights or a busy household → earlier; a cold night → later. It always comes on by ${smart.latest}, and it adds to the load once on.`;
}

export function buildPrompt(input) {
  const usable = Math.max(input.soc - input.floor_soc, 0);
  const onBattery = input.on_battery
    ? `The house has been running on battery ("sbg") since ${input.on_battery_since ?? "earlier tonight"}.${
        input.previous_plan
          ? ` Previous plan: keep a ${input.previous_plan.reserve_soc}% reserve, because "${input.previous_plan.reason}".`
          : ""
      } Re-evaluate that plan with the fresh numbers below.`
    : `The house is currently on grid ("solar" mode — at night this means the grid powers the load and the battery is untouched).`;

  const routine =
    input.history_nights > 0
      ? `Learned from the last ${input.history_nights} recorded night(s):
- Typical load at this hour: ${n(input.typical_load_now_w, " W")} (right now it is ${n(input.load_w, " W")}).
- The house usually goes quiet by: ${input.quiet_by ?? "no clear drop found"}.
- Typical load for the rest of tonight:
${lines(input.typical_hourly_load, (h) => `  ${h.hour} ${n(h.load_w, " W")}`, "  (none)")}`
      : `No household history recorded yet. Infer the routine from the time and the current load (a household is usually most active in the evening and quieter after bedtime), and prefer a shorter recheck so the measured draw can correct you.`;

  return `You manage a home solar inverter's output mode overnight in ${input.month}, at latitude ${input.latitude}°. It is ${input.now}.

Output modes (priority order for powering the house):
- "sbg": solar, then battery, then grid. At night that means the house runs from the battery, with the grid only as a fallback.
- "solar": solar, then grid, then battery (only if both are gone). At night the grid powers the house and the battery is kept for outages.

Instead of an all-or-nothing call, you set a RESERVE: the house runs on battery until the battery reaches reserve_soc, then the controller automatically switches to "solar" (grid) for the rest of the night. It is completely fine if the battery would not last until sunrise — that is what the reserve is for.

Goals, in order:
1. Never go below the hard floor of ${input.floor_soc}%.
2. Keep enough backup for load-shedding (grid outages) later tonight and early tomorrow. Size it from the recent outage history below; if outages have been frequent or long, keep more.
3. Don't drain deeper than tomorrow's sun can refill. If tomorrow looks as sunny as recent days on which the battery filled, a lower reserve is fine. If tomorrow looks cloudy or rainy, keep more so the battery isn't left low for days.
4. Within those limits, save as many grid units as possible by running on battery.

${onBattery}

Battery now:
- SOC: ${n(input.soc, "%")} (${n(usable, "%")} above the floor), capacity ${n(input.rated_capacity_ah, " Ah")}, ${n(input.battery_v, " V", 1)}.
- Draw: ${n(input.discharge_a, " A", 1)} (${input.discharge_measured ? "measured from the battery" : "estimated from the house load"}); load ${n(input.load_w, " W")}, PV ${n(input.pv_w, " W")}.
- Sunrise ${input.sunrise}; ${n(input.hours_until_sunrise, " h", 1)} until the sun can take over again (including ramp-up).
- Each hour of backup at the typical sleeping load costs about ${n(input.soc_per_backup_hour, "% SOC", 1)}.

Household routine:
${routine}

Projected SOC if the house stays on battery from now (${input.trajectory_basis === "history" ? "current load for this hour, then the learned routine" : "assuming today's current load all night"} — use these numbers, don't redo the arithmetic):
${lines(input.trajectory, (p) => `  ${p.time} ${n(p.soc, "%")}`, "  (not available)")}

Grid outages in the last 7 days:
${lines(input.outages, (o) => `- ${o.when}, ${n(o.minutes, " min")}`, "- none recorded")}

Weather and season:
${weatherSection(input.weather)}
Cooler nights than recent ones usually mean less fan/AC load than the learned routine; warmer means more.

${smartLoadSection(input.smart_load)}

Decide:
- mode: "sbg" to run on battery now (until reserve_soc), or "solar" to stay on grid for now (e.g. SOC is already at or near the reserve you'd want, or it's better to wait).
- reserve_soc: the SOC to stop at and switch to grid, between ${input.floor_soc} and 95.
- recheck_minutes: when to look again (15–120). Check sooner when the load is changing (family still awake, no history yet) or the battery is close to the reserve; later when things are steady.
- smart_load_on_at: "HH:MM" (24-hour) for when the smart load should come on tonight.
- reason: one or two short sentences naming the factors that drove the reserve (routine, outages, tomorrow's sun, season). Speak plainly to the homeowner.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: NIGHT_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

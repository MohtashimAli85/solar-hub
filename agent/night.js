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

function weatherSection(weather, comingDay) {
  if (!weather) {
    return `Forecast unavailable right now — assume ${comingDay}'s sun is uncertain and keep a larger reserve.`;
  }
  const next = weather.next_day;
  const nextLine = next
    ? `The sun that will next charge the battery is ${next.relative}'s (${next.date}): ${n(next.radiation_kwh_m2, " kWh/m²", 1)} radiation, ${n(next.sunshine_h, " h", 1)} sunshine, ${n(next.cloud_pct, "%")} daytime cloud, ${n(next.rain_prob_pct, "%")} rain chance.`
    : `No forecast for ${comingDay}.`;
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
  const state =
    smart.currently_on == null ? "unknown" : smart.currently_on ? "ON (enabled — heavy loads cut)" : "OFF (disabled — everything running)";
  if (!smart.choose_on_time) {
    return `Smart load is currently ${state}. It is ${smart.season}, so the controller enables it at ${smart.earliest} — return "${smart.earliest}" for smart_load_on_at.`;
  }
  return `Smart load is currently ${state}. It is ${smart.season}: choose when to enable it tonight (cutting heavy and non-UPS loads), between ${smart.earliest} and ${smart.latest}. Enable it earlier when the battery needs protecting — a warm night with heavy cooling loads, a busy household, a tight reserve, or a long night ahead; later when the night is cool and the battery has room. The controller enables it by ${smart.latest} at the latest.`;
}

function recentNightsSection(nights) {
  if (!nights || nights.length === 0) return "No earlier nights recorded yet.";
  return lines(
    nights,
    (night) => {
      const battery = night.battery_from
        ? `on battery ${night.battery_from}–${night.battery_until ?? "?"} from ${n(night.soc_at_battery_start, "%")}, evening draw ${n(night.evening_draw_a, " A", 1)}, sleeping draw ${n(night.sleep_draw_a, " A", 1)}`
        : "not on battery";
      const next = night.next_day_full_at
        ? `next day full by ${night.next_day_full_at}`
        : night.next_day_max_soc != null
          ? `next day peaked at ${n(night.next_day_max_soc, "%")}`
          : "next day not recorded";
      return `- ${night.night}: ${battery}; lowest ${n(night.lowest_soc, "%")}, morning ${n(night.morning_soc, "%")}; ${next}.`;
    },
    "No earlier nights recorded yet.",
  );
}

function eveningSection(input) {
  if (!input.before_usual_start) return "";
  const waiting =
    input.sunrise_soc_if_waiting == null
      ? ""
      : ` If you wait until ${input.usual_night_start} and then run on battery for the rest of the night, it still holds about ${n(input.sunrise_soc_if_waiting, "%")} at sunrise (the floor is ${input.floor_soc}%). Whatever that leaves above the reserve you want is battery that goes unused unless you start earlier; if it's at or below that reserve, waiting is right.`;
  return `It is the evening, before the usual night start at ${input.usual_night_start}.${waiting} You may start the battery early — choose "sbg" now — when the battery is high (around 80–90% or more), the projection below shows it can carry the busy evening (usually 10–20 A until bedtime) and then the quiet night (around 4 A) down to a sensible reserve, and ${input.coming_day}'s sun looks able to refill it. Look at how recent nights went: if they reached the morning with plenty left and the next day still filled the battery, starting earlier is the better use of it. Otherwise choose "solar" and a recheck_minutes that brings you back later this evening; the controller asks again by ${input.usual_night_start} at the latest.

`;
}

export function buildPrompt(input) {
  const usable = Math.max(input.soc - input.floor_soc, 0);
  const onBattery = input.on_battery
    ? `The house has been running on battery ("sbg") since ${input.on_battery_since ?? "earlier tonight"}.${
        input.previous_plan
          ? ` Previous plan: keep a ${input.previous_plan.reserve_soc}% reserve, because "${input.previous_plan.reason}".`
          : ""
      } Re-evaluate that plan with the fresh numbers below.`
    : `The inverter is currently in "solar" mode — at night the grid powers the house and the battery is kept.`;

  const routine =
    input.history_nights > 0
      ? `Learned from the last ${input.history_nights} recorded night(s):
- Typical load at this hour: ${n(input.typical_load_now_w, " W")} (right now it is ${n(input.load_w, " W")}).
- The house usually goes quiet by: ${input.quiet_by ?? "no clear drop found"}.
- Typical load for the rest of tonight:
${lines(input.typical_hourly_load, (h) => `  ${h.hour} ${n(h.load_w, " W")} (≈${n(h.draw_a, " A", 1)} from the battery)`, "  (none)")}`
      : `No household history recorded yet. Infer the routine from the time and the current load (a household is usually most active in the evening and quieter after bedtime), and prefer a shorter recheck so the measured draw can correct you.`;

  return `Night decision for ${input.month}, latitude ${input.latitude}°. It is ${input.now}. At night "sbg" means the house runs from the battery; "solar" means the grid powers it and the battery is kept.

Instead of an all-or-nothing call, you set a RESERVE: the house runs on battery until the battery reaches reserve_soc, then the controller automatically switches to "solar" (grid) for the rest of the night. It is completely fine if the battery would not last until sunrise — that is what the reserve is for.

Tonight the hard floor is ${input.floor_soc}%. Even after the switch to "solar", the inverter itself keeps drawing about ${n(input.standby_drain_a, " A", 1)} from the battery until the sun is up, so right now the lowest reserve that still keeps the battery at ${input.floor_soc}% by morning is ${input.min_reserve_soc}% — the controller never stops lower than that. Size the load-shedding backup from the recent outage history below (frequent or long outages → keep more). If ${input.coming_day} looks as sunny as recent days on which the battery filled, a lower reserve is fine; if it looks cloudy or rainy, keep more so the battery isn't left low for days.

${onBattery}

Battery now:
- SOC: ${n(input.soc, "%")} (${n(usable, "%")} above the floor), capacity ${n(input.rated_capacity_ah, " Ah")}, ${n(input.battery_v, " V", 1)}.
- Draw: ${n(input.discharge_a, " A", 1)} (${input.discharge_measured ? "measured from the battery" : "estimated from the house load"}); load ${n(input.load_w, " W")}, PV ${n(input.pv_w, " W")}.
- Sunrise ${input.sunrise}; ${n(input.hours_until_sunrise, " h", 1)} until the sun can take over again (including ramp-up).
- Each hour of backup at the typical sleeping load costs about ${n(input.soc_per_backup_hour, "% SOC", 1)}.

${eveningSection(input)}Household routine:
${routine}

How recent nights went (battery use, draw, and whether the next day's sun refilled it):
${recentNightsSection(input.recent_nights)}

Projected SOC if the house stays on battery from now (${input.trajectory_basis === "history" ? "current load for this hour, then the learned routine" : "assuming today's current load all night"} — use these numbers, don't redo the arithmetic):
${lines(input.trajectory, (p) => `  ${p.time} ${n(p.soc, "%")}`, "  (not available)")}

Grid outages in the last 7 days:
${lines(input.outages, (o) => `- ${o.when}, ${n(o.minutes, " min")}`, "- none recorded")}

Weather and season:
${weatherSection(input.weather, input.coming_day)}
Cooler nights than recent ones usually mean less fan/AC load than the learned routine; warmer means more.

${smartLoadSection(input.smart_load)}

Decide:
- mode: "sbg" to run on battery now (until reserve_soc), or "solar" to stay in Solar mode for now, with the grid powering the house (e.g. SOC is already at or near the reserve you'd want, or it's better to wait).
- reserve_soc: the SOC at which to switch back to "solar" mode, between ${input.min_reserve_soc} and 95.
- recheck_minutes: when to look again (15–120). Check sooner when the load is changing (family still awake, no history yet) or the battery is close to the reserve; later when things are steady.
- smart_load_on_at: "HH:MM" (24-hour) for when smart load should be enabled tonight (heavy loads cut).
- reason: one or two short sentences naming the factors that drove the reserve (routine, outages, ${input.coming_day}'s sun, season). Refer to that day as "${input.coming_day}". Speak plainly to the homeowner.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: NIGHT_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

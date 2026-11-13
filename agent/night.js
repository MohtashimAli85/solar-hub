import { runAgent, isMainModule } from "./lib.js";

export const NIGHT_SCHEMA = {
  type: "object",
  properties: {
    mode: { type: "string", enum: ["sbg", "solar"] },
    reserve_soc: { type: "number" },
    recheck_minutes: { type: "number" },
    confidence: { type: "number" },
    reason: { type: "string" },
  },
  required: ["mode", "reserve_soc", "recheck_minutes", "confidence", "reason"],
};

const n = (value, unit = "", digits = 0) =>
  value === null || value === undefined ? "unknown" : `${Number(value).toFixed(digits)}${unit}`;

function lines(rows, format, empty) {
  return rows && rows.length ? rows.map(format).join("\n") : empty;
}

function dayLine(d) {
  const refill = d.full_at
    ? `full again by ${d.full_at}`
    : d.max_soc != null
      ? `only reached ${n(d.max_soc, "%")}`
      : "refill not recorded";
  const start = d.morning_low_soc != null ? `battery started the morning at ${n(d.morning_low_soc, "%")}` : "morning start not recorded";
  return `${d.date}: ${n(d.radiation_kwh_m2, " kWh/m²", 1)} radiation, ${n(d.cloud_pct, "%")} cloud, ${n(d.rain_prob_pct, "%")} rain chance → ${start}, ${refill}`;
}

function refillSection(refill) {
  if (!refill) return "No recorded refills yet.";
  const latest = refill.latest_full_at ? `, at the latest by ${refill.latest_full_at}` : "";
  const lowest = refill.lowest_start_filled != null ? ` The lowest morning start that still filled was ${n(refill.lowest_start_filled, "%")}.` : "";
  const darkest = refill.darkest_full ? `\n- Darkest day that still filled: ${dayLine(refill.darkest_full)}.` : "";
  const wettest = refill.wettest_full && refill.wettest_full.date !== refill.darkest_full?.date ? `\n- Rainiest-forecast day that still filled: ${dayLine(refill.wettest_full)}.` : "";
  return `The battery was full again on ${refill.full_days} of the last ${refill.days} recorded days${latest}.${lowest}${darkest}${wettest}`;
}

function weatherSection(weather, comingDay) {
  if (!weather) {
    return `Forecast unavailable right now, so ${comingDay}'s sun is uncertain: lean on the recent nights below, and keep a few % more only if they show refills were tight.`;
  }
  const next = weather.next_day;
  const nextLine = next
    ? `${next.relative} (${next.date}): ${n(next.radiation_kwh_m2, " kWh/m²", 1)} radiation (last 7 days averaged ${n(weather.recent_avg_radiation_kwh_m2, " kWh/m²", 1)}), ${n(next.sunshine_h, " h", 1)} sunshine, ${n(next.cloud_pct, "%")} daytime cloud, ${n(next.rain_prob_pct, "%")} rain chance.${weather.expected_pv_kwh_next_day != null ? ` About ${n(weather.expected_pv_kwh_next_day, " kWh", 1)} of solar expected.` : ""}`
    : `No forecast for ${comingDay}.`;
  return `The sun that will next charge the battery — ${nextLine}

Refill evidence (what the sun actually did on recent days):
${refillSection(weather.refill)}
${lines(weather.recent_days, (d) => `- ${dayLine(d)}`, "- (no recent days recorded yet)")}

Tonight's forecast low: ${n(weather.tonight_min_temp_c, "°C", 1)} vs ${n(weather.recent_nights_min_temp_c, "°C", 1)} average over the last week's nights. Cooler nights than recent ones usually mean less fan/AC load than the learned routine; warmer means more.`;
}

function outageSection(outages) {
  if (!outages || outages.length === 0) return "- none recorded";
  return lines(
    outages,
    (o) =>
      o.before_sun_hours > 0
        ? `- ${o.when}, ${n(o.minutes, " min")}: ${n(o.before_sun_hours, " h", 1)} of it between midnight and the sun taking over → a ${n(o.reserve_to_cover, "%")} reserve would have carried it.`
        : `- ${o.when}, ${n(o.minutes, " min")}: before midnight or in daylight, when the battery is still high or the sun carries the house; the reserve isn't needed for it.`,
    "- none recorded",
  );
}

function smartLoadSection(smart) {
  if (!smart) return "";
  const state =
    smart.currently_on == null ? "unknown" : smart.currently_on ? "ON (enabled — heavy loads cut)" : "OFF (disabled — everything running)";
  const rule =
    smart.trigger_amps > 0
      ? `The controller enables it on its own if the battery drain stays at or above ${n(smart.trigger_amps, " A")} for ${smart.trigger_minutes} minutes; short spikes don't count.`
      : "The controller doesn't enable it at night.";
  return `Smart load is currently ${state}. ${rule} You don't decide it.`;
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
    ? `The house has been running on battery ("sbg") since ${input.on_battery_since ?? "earlier tonight"}.`
    : `The inverter is currently in "solar" mode — at night the grid powers the house and the battery is kept.`;
  const previous = input.previous_plan
    ? ` Your previous plan tonight: a ${input.previous_plan.reserve_soc}% reserve, because "${input.previous_plan.reason}". Keep that reserve unless a fact below has really changed (a new outage, a clearly different forecast, the load running well above or below the routine); don't move it by a few % between checks.`
    : "";

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

Tonight the hard floor is ${input.floor_soc}%. Even after the switch to "solar", the inverter itself keeps drawing about ${n(input.standby_drain_a, " A", 1)} from the battery until the sun is up, so right now the lowest reserve that still keeps the battery at ${input.floor_soc}% by morning is ${input.min_reserve_soc}% — the controller never stops lower than that. The battery is never charged from the grid: only the sun refills it, and the family wants it back to 100% every day. Every % the battery carries tonight is grid power saved.

How to choose reserve_soc — go through these in order:
1. Start at ${input.min_reserve_soc}%.
2. Outage backup. The reserve is what carries the house if the grid goes off after the switch to "solar". Each outage below says how many hours of it fell between midnight and the sun taking over (by then the battery is usually down at its reserve) and what reserve would have carried it. Cover the usual night outage of the last week; a single rare very long one doesn't have to be covered in full. Evening and daytime outages don't need the reserve.
3. Refill check. Go higher than step 2 only if ${input.coming_day}'s sun might not bring the battery back to 100% from that level. Judge it from the refill evidence: if recent days with similar or lower radiation, and similar or higher rain chance, still filled from a similar or lower morning start, the refill is not a reason to keep more. Cloud % and rain chance on their own are not evidence — the radiation figure and the recorded refills are. Keep more only when ${input.coming_day}'s radiation is clearly below every recent day that filled, or recent days didn't fill.
4. Use the larger of steps 2 and 3, as a whole number.

${onBattery}${previous}

Battery now:
- SOC: ${n(input.soc, "%")} (${n(usable, "%")} above the floor), capacity ${n(input.rated_capacity_ah, " Ah")}, ${n(input.battery_v, " V", 1)}.
- Draw: ${n(input.discharge_a, " A", 1)} (${input.discharge_measured ? "measured from the battery" : "estimated from the house load"}); load ${n(input.load_w, " W")}, PV ${n(input.pv_w, " W")}.
- Sunrise ${input.sunrise}; ${n(input.hours_until_sunrise, " h", 1)} until the sun can take over again (including ramp-up).
- Each hour of backup at the typical sleeping load costs about ${n(input.soc_per_backup_hour, "% SOC", 1)}.

${eveningSection(input)}Household routine:
${routine}

How recent nights went (battery use, draw, and the next day's refill):
${recentNightsSection(input.recent_nights)}

Projected SOC if the house stays on battery from now (${input.trajectory_basis === "history" ? "current load for this hour, then the learned routine" : "assuming today's current load all night"} — use these numbers, don't redo the arithmetic):
${lines(input.trajectory, (p) => `  ${p.time} ${n(p.soc, "%")}`, "  (not available)")}

Grid outages in the last 7 days:
${outageSection(input.outages)}

Weather and season:
${weatherSection(input.weather, input.coming_day)}

${smartLoadSection(input.smart_load)}

Decide:
- mode: "sbg" to run on battery now (until reserve_soc), or "solar" to stay in Solar mode for now, with the grid powering the house (e.g. SOC is already at or near the reserve you'd want, or it's better to wait).
- reserve_soc: the SOC at which to switch back to "solar" mode, between ${input.min_reserve_soc} and 95.
- recheck_minutes: when to look again (15–120). Check sooner when the load is changing (family still awake, no history yet) or the battery is close to the reserve; later when things are steady.
- reason: one or two short sentences with the numbers that set the reserve: which outage it covers, and whether ${input.coming_day}'s sun can refill from it (compared with a recent day that did). Refer to that day as "${input.coming_day}". Speak plainly to the homeowner.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: NIGHT_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

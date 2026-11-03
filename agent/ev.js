import { runAgent, isMainModule } from "./lib.js";

export const EV_SCHEMA = {
  type: "object",
  properties: {
    charge_today: { type: "string", enum: ["yes", "if_spare", "no"] },
    recheck_minutes: { type: "number" },
    reason: { type: "string" },
  },
  required: ["charge_today", "recheck_minutes", "reason"],
};

const n = (value, unit = "", digits = 0) =>
  value === null || value === undefined ? "unknown" : `${Number(value).toFixed(digits)}${unit}`;

const BASIS = {
  read: "read from the scooter over Bluetooth (the value it held when last switched off)",
  live: "read live from the scooter",
  estimated: "estimated from the charger running since the last reading",
  full: "the charger switched itself off, so it's full",
};

function todayTable(rows) {
  if (!rows || rows.length === 0) return "  (no sun left today)";
  return rows.map((r) => `  ${r.time.padStart(8)}  ${String(n(r.spare_w, " W")).padStart(7)} spare`).join("\n");
}

function daysTable(days) {
  if (!days || days.length === 0) return "- Forecast unavailable for the coming days.";
  return days
    .map(
      (d) =>
        `- ${d.relative} (${d.date}): ${n(d.radiation_kwh_m2, " kWh/m²", 1)} radiation, ${n(d.sunshine_h, " h", 1)} sunshine, ${n(d.cloud_pct, "%")} daytime cloud, ${n(d.rain_prob_pct, "%")} rain chance → about ${n(d.expected_pv_kwh, " kWh", 1)} of solar, ${n(d.expected_spare_kwh, " kWh", 1)} spare after the house, battery, geyser and water tank`,
    )
    .join("\n");
}

export function buildPrompt(input) {
  return `Scooter charging decision. It is ${input.now}; sunset ${input.sunset}, ${n(input.hours_of_sun_left, " h", 1)} of sun left.

The family's electric scooter (Yadea T5L, ${n(input.battery_kwh, " kWh", 2)} LFP battery) charges from a ${n(input.charger_w, " W")} charger on the house's solar. The homeowner's rule: the scooter charges from SOLAR ONLY, never from the grid. A normal day's riding uses about ${n(input.daily_use_pct, "%")} of its battery. It charges to 100% (LFP, so a full charge is fine).

The scooter now:
- ${n(input.soc, "%")} — ${BASIS[input.soc_basis] ?? input.soc_basis}.
- About ${n(input.days_of_riding_left, " days", 1)} of normal riding left.
- ${n(input.hours_to_full, " h", 1)} of charging (${n(input.need_kwh, " kWh", 1)}) to reach 100%.
- ${input.charging_now ? "The charger is running right now." : "It is not charging right now."}

Today's sun (use these numbers, don't redo the arithmetic):
- About ${n(input.spare_kwh_today, " kWh", 1)} of spare solar is left today after the water tank and the geyser, with the house battery still reaching what tonight needs by sunset.
- ${input.other_loads_today}
- Spare solar by hour (forecast minus the usual house load, before the other loads):
${todayTable(input.today_hours)}

The coming days:
${daysTable(input.next_days)}

Decide charge_today:
- "yes": charge today ahead of the geyser, because waiting is worse — the coming days look too weak to cover what the scooter needs, or it's running low (under about two days of riding). Example: 80% but tomorrow and the day after are cloudy → "yes", top it up while the sun is here.
- "if_spare": charge today only from what's left after the geyser and water tank. Use this when the coming days also look good enough, or the scooter has plenty left.
- "no": don't remind today — it's full or nearly full and the coming days are sunny, or today has no spare solar worth using.
recheck_minutes: when to look again (30–240). Sooner when the sky is changing or the charge is low.
reason: one or two plain sentences for the family naming what actually decided it, e.g. "At 80%, but tomorrow looks cloudy — topping up while today's sun lasts." Write times in 12-hour form.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: EV_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

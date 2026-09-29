import { runAgent, isMainModule } from "./lib.js";

export const DAY_SCHEMA = {
  type: "object",
  properties: {
    mode: { type: "string", enum: ["sbg", "solar"] },
    recheck_minutes: { type: "number" },
    confidence: { type: "number" },
    reason: { type: "string" },
  },
  required: ["mode", "recheck_minutes", "confidence", "reason"],
};

const n = (value, unit = "", digits = 0) =>
  value === null || value === undefined ? "unknown" : `${Number(value).toFixed(digits)}${unit}`;

function historyTable(rows) {
  if (!rows || rows.length === 0) return "  (no samples recorded yet)";
  const header = "  time   PV W  load W  battery A  mode";
  const body = rows
    .map((r) => `  ${r.time}  ${String(n(r.pv_w)).padStart(5)}  ${String(n(r.load_w)).padStart(6)}  ${String(n(r.battery_a, "", 1)).padStart(9)}  ${r.mode ?? "?"}`)
    .join("\n");
  return `${header}\n${body}`;
}

function forecastSection(input) {
  if (!input.forecast_available) return "Forecast unavailable right now.";
  if (!input.forecast.length) return "No forecast hours left before sunset.";
  return input.forecast
    .map((h) => `  ${h.time} ${n(h.radiation_w_m2, " W/m²")} radiation, ${n(h.cloud_pct, "%")} cloud`)
    .join("\n");
}

function recentDays(days) {
  return days && days.length
    ? days
        .map(
          (d) =>
            `- ${d.date}: ${n(d.radiation_kwh_m2, " kWh/m²", 1)} → ${d.full_at ? `battery full by ${d.full_at}` : d.max_soc != null ? `battery peaked at ${n(d.max_soc, "%")}` : "not recorded"}`,
        )
        .join("\n")
    : "- (no recent days recorded yet)";
}

function batteryOutlook(input) {
  const sunset =
    input.projected_soc_at_sunset == null
      ? "Projection to sunset unavailable."
      : input.projected_full_at
        ? `On pace to be full by ${input.projected_full_at} (about ${n(input.projected_soc_at_sunset, "%")} at sunset).`
        : `About ${n(input.projected_soc_at_sunset, "%")} by sunset.`;
  const night =
    input.tonight_needs_soc == null
      ? ""
      : ` A normal night from ${input.usual_night_start} to sunrise uses about ${n(input.tonight_needs_soc, "% SOC")}${
          input.projected_sunrise_soc == null
            ? ""
            : `, which would leave about ${n(input.projected_sunrise_soc, "%")} at sunrise (the floor is ${input.floor_soc}%)`
        }.`;
  return sunset + night;
}

function switchesSection(input) {
  const rows = input.today_switches?.length
    ? input.today_switches.map((s) => `- ${s.time} → ${s.mode}: ${s.reason}`).join("\n")
    : "- none yet today";
  const last =
    input.minutes_since_last_switch == null ? "" : ` The last switch was ${input.minutes_since_last_switch} minutes ago.`;
  return `${rows}\n${input.switches_today} of the 4 allowed switches used today.${last}`;
}

function loadNote(input) {
  if (input.load_w == null || input.typical_load_now_w == null) return "";
  return input.load_is_unusual
    ? ` That is well above the usual ${n(input.typical_load_now_w, " W")} for this hour — probably a temporary big load such as an EV or the oven.`
    : ` (Usual for this hour: ${n(input.typical_load_now_w, " W")}.)`;
}

export function buildPrompt(input) {
  return `Daytime decision. It is ${input.now}; sunset ${input.sunset}, ${n(input.hours_of_sun_left, " h", 1)} of sun left. The inverter is on ${input.current_mode} mode. Choose "sbg" to stay on the battery-first mode or "solar" for Solar mode (solar, then grid, then battery).

The controller's fixed rules did not settle this one: the sky has been ${input.sky} for ${input.dim_minutes} minutes and the forecast is not clear-cut. "grey" means some sun but the battery is draining; "dark" means almost no sun and the battery is draining.

Now:
- Battery ${n(input.soc, "%")} of ${n(input.rated_capacity_ah, " Ah")}, ${n(input.battery_v, " V", 1)}, current ${n(input.battery_a, " A", 1)} (positive charging, negative draining).
- PV ${n(input.pv_w, " W")}, house load ${n(input.load_w, " W")}.${loadNote(input)}
- Smart load: ${input.smart_load_on == null ? "unknown" : input.smart_load_on ? "ON (heavy loads cut)" : "OFF"}.

Last 90 minutes, oldest first (a short drain that recovers is a panel ramp-up, not a dark sky):
${historyTable(input.last_90_min)}

Forecast until sunset:
${forecastSection(input)}

How recent days went (radiation → how the battery charged):
${recentDays(input.recent_days)}

Battery outlook (use these numbers, don't redo the arithmetic):
${batteryOutlook(input)}

Today's switches:
${switchesSection(input)}

Decide:
- mode: "sbg" unless the sun is really gone for long enough, or the battery won't hold what tonight needs and the day won't refill it. A ramp-up drain, a cloud that the forecast says clears, or a comfortable battery means "sbg".
- recheck_minutes: when to look again (15–60). Sooner when the sky is changing, later when it is settled.
- confidence: 0 to 1.
- reason: one or two plain sentences naming the actual factors, e.g. "Dark for 40 minutes and no sun forecast until 15:00". Say "Solar mode", never "grid mode".`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: DAY_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

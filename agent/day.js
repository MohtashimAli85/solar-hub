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

const METHODS = {
  pv_array: "expected PV from the array size minus the typical house load, capped at the inverter's max charge current",
  pv_headroom: "the PV this hour scaled by the forecast sun, minus the typical house load, capped at the inverter's max charge current — i.e. what the battery could take once any unusual load stops",
  charge_scaling: "today's measured charge current scaled by the forecast sun for each hour",
  constant_charge: "today's charge current held constant (no forecast available)",
};

function loadNote(input) {
  if (input.load_w == null || input.typical_load_now_w == null) return "";
  if (input.load_w > input.typical_load_now_w * 1.5 + 300) {
    return ` That is well above the usual ${n(input.typical_load_now_w, " W")} for this hour — probably a temporary big load (for example an EV charging) soaking up the sun right now.`;
  }
  return ` (Usual for this hour: ${n(input.typical_load_now_w, " W")}.)`;
}

export function buildPrompt(input) {
  const hours = input.next_hours?.length
    ? input.next_hours
        .map((h) => `  ${h.time} ${n(h.radiation_w_m2, " W/m²")} radiation, ${n(h.cloud_pct, "%")} cloud`)
        .join("\n")
    : "  (no forecast)";
  const recent = input.recent_days?.length
    ? input.recent_days
        .map(
          (d) =>
            `- ${d.date}: ${n(d.radiation_kwh_m2, " kWh/m²", 1)} → ${d.full_at ? `full by ${d.full_at}` : d.max_soc != null ? `peaked at ${n(d.max_soc, "%")}` : "not recorded"}`,
        )
        .join("\n")
    : "- (no recent days recorded yet)";

  return `You manage a home solar inverter's output mode during the day. It is ${input.now}.

Output modes (priority order for powering the house):
- "sbg": solar, then battery, then grid. Whatever the sun doesn't cover comes out of the battery. Fine only while the sun is actually charging the battery and will fill it by sunset.
- "solar": solar, then grid, then battery (only if both solar and grid are gone). The grid covers any shortfall, so the battery is kept and all spare solar goes into it.

The homeowner's rule of thumb: even a modest charge current (around 10 A) fills the battery before sunset when there is real sun. Only switch to "solar" when it clearly won't fill.

Battery now: ${n(input.soc, "%")} of ${n(input.rated_capacity_ah, " Ah")} (${n(input.ah_to_full, " Ah")} to full), charging at ${n(input.charge_a, " A", 1)}${input.max_charge_a != null ? ` — the inverter can charge it at up to ${n(input.max_charge_a, " A")} when there is enough sun` : ""}. PV ${n(input.pv_w, " W")}, load ${n(input.load_w, " W")}.${loadNote(input)} Current mode: ${input.current_mode}.

${input.battery_draining ? "RIGHT NOW THE BATTERY IS BEING DRAINED on sbg — the sun isn't covering the house. That is a strong reason for \"solar\", even when the battery is nearly full: a full battery is only useful if it is still full tonight.\n\n" : ""}A low charge current right now is not a problem by itself: if a big load is only temporary, the battery catches up quickly afterwards at the higher rate. Judge by whether the battery can still be full by sunset.

Sunset ${input.sunset}, ${n(input.hours_of_sun_left, " h", 1)} of sun left.
Projected SOC at sunset: ${n(input.projected_soc_at_sunset, "%")}${input.projected_full_at ? ` (full around ${input.projected_full_at})` : ""} — computed from ${METHODS[input.projection_method] ?? input.projection_method}. Use it rather than your own arithmetic.

Forecast for the rest of the day${input.forecast_available ? "" : " (unavailable)"}:
${hours}

How recent days actually went:
${recent}

Judge the borderline cases: a passing cloud that clears soon, a projection just under 100% on an otherwise sunny afternoon, or a cloudy morning that the forecast says will clear. Decide:
- mode: "sbg" or "solar".
- recheck_minutes: when to look again (15–120).
- reason: one short, plain sentence for the homeowner.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: DAY_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

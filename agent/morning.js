import { runAgent, isMainModule } from "./lib.js";

export const MORNING_SCHEMA = {
  type: "object",
  properties: {
    mode: { type: "string", enum: ["sbg", "solar"] },
    confidence: { type: "number" },
    reason: { type: "string" },
  },
  required: ["mode", "confidence", "reason"],
};

export function buildPrompt(input) {
  const {
    soc,
    charge_a,
    charge_threshold_a,
    pv_w,
    expected_pv_w,
    load_w,
    mode,
    minutes_since_sunrise,
  } = input;

  return `You control a home solar inverter's output priority in the morning: either "sbg" (solar + battery + grid, tops up the battery from the grid too) or "solar" (solar only).

The user's rule: if the battery is charging at more than ${charge_threshold_a} A, use "sbg"; otherwise use "solar".

Current reading, ${minutes_since_sunrise} minutes after sunrise:
- Battery SOC: ${soc ?? "unknown"}%
- Battery charge current: ${charge_a ?? "unknown"} A (threshold: ${charge_threshold_a} A)
- PV generation: ${pv_w ?? "unknown"} W
- Expected clear-sky PV at this sun angle: ${expected_pv_w ?? "unknown"} W
- Load: ${load_w ?? "unknown"} W
- Current output mode: ${mode ?? "unknown"}

Apply the user's rule using the charge current. Use actual PV versus expected PV only to explain *why* the charge current is where it is: if actual PV is close to expected, the sun is just still low and it will improve on its own; if actual PV is well below expected, it's likely cloudy and won't improve soon. Give a short reason that reflects this.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: MORNING_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

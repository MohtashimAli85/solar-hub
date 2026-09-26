import { runAgent, projectRuntimeHours, runtimeText, isMainModule } from "./lib.js";

export const DECISION_SCHEMA = {
  type: "object",
  properties: {
    mode: { type: "string", enum: ["sbg", "solar"] },
    confidence: { type: "number" },
    reason: { type: "string" },
  },
  required: ["mode", "confidence", "reason"],
};

export function buildPrompt(telemetry) {
  const {
    soc,
    discharge_a,
    usable_capacity_ah,
    pv_w,
    load_w,
    mode,
    hour,
    required_hours,
  } = telemetry;

  const runtimeHours = projectRuntimeHours(usable_capacity_ah, discharge_a);

  return `You control a home solar inverter's output priority at night: either "sbg" (solar + battery + grid, draws from the battery) or "solar" (solar only, does not draw from the battery).

Current reading:
- Hour: ${hour}:00
- Battery SOC: ${soc ?? "unknown"}%
- Usable battery capacity remaining: ${usable_capacity_ah} Ah
- Battery discharge current: ${discharge_a} A
- Projected runtime at this discharge rate (already computed for you): ${runtimeText(runtimeHours)}
- PV generation: ${pv_w ?? "unknown"} W
- Load: ${load_w ?? "unknown"} W
- Current output mode: ${mode ?? "unknown"}
- Hours until sunrise/target hour (must cover this on the battery if SBG is not used): ${required_hours}

Use the projected runtime above, not your own arithmetic, to judge coverage. Decide "sbg" only if the projected runtime plausibly covers ${required_hours} hours without dropping to an unsafe level. Otherwise decide "solar". Give a short reason.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: DECISION_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

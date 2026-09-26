import { runAgent, projectRuntimeHours, runtimeText, isMainModule } from "./lib.js";

export const VERIFY_SCHEMA = {
  type: "object",
  properties: {
    verified: { type: "boolean" },
    confidence: { type: "number" },
    reason: { type: "string" },
  },
  required: ["verified", "confidence", "reason"],
};

export function buildPrompt(input) {
  const {
    original_reason,
    soc,
    usable_capacity_ah,
    verified_discharge_a,
    required_hours,
  } = input;

  const runtimeHours = projectRuntimeHours(usable_capacity_ah, verified_discharge_a);

  return `Earlier tonight, a decision was made to switch this home solar inverter to "sbg" mode (draws from the battery), for this reason: "${original_reason}"

That decision was based on an estimate. Now real samples of actual discharge current have been measured since the switch. Verify whether the decision still holds:

- Battery SOC: ${soc ?? "unknown"}%
- Usable battery capacity remaining: ${usable_capacity_ah} Ah
- Measured (real, not estimated) discharge current: ${verified_discharge_a} A
- Projected runtime at this measured rate (already computed for you): ${runtimeText(runtimeHours)}
- Hours still required until sunrise/target hour: ${required_hours}

Use the projected runtime above, not your own arithmetic. Set "verified" to true only if the measured runtime plausibly covers the required hours (a small shortfall on a high SOC is still acceptable). Set it to false if the battery is draining faster than the original estimate assumed and won't make it — in that case this decision will be revisited from scratch. Give a short reason either way.`;
}

if (isMainModule(import.meta.url)) {
  runAgent({ schema: VERIFY_SCHEMA, buildPrompt }).catch((error) => {
    console.error(error);
    process.exit(1);
  });
}

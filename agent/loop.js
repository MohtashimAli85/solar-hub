import { generateStructured, isMainModule } from "./lib.js";
import { DECISION_SCHEMA, buildPrompt as buildDecidePrompt } from "./decide.js";
import { VERIFY_SCHEMA, buildPrompt as buildVerifyPrompt } from "./verify.js";

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

async function decide(telemetry) {
  return generateStructured({ schema: DECISION_SCHEMA, prompt: buildDecidePrompt(telemetry) });
}

async function verify(input) {
  return generateStructured({ schema: VERIFY_SCHEMA, prompt: buildVerifyPrompt(input) });
}

// Runs decide -> (if sbg) verify against each real probe, in order, re-deciding
// from scratch whenever a probe fails verification. Stops as soon as a probe
// verifies (no more checks needed) or a re-decide reverts to solar.
export async function runLoop({ initial, probes }) {
  console.error("Deciding based on initial (estimated) telemetry...");
  let decision = await decide(initial);
  console.error("Decision:", decision);

  if (decision.mode !== "sbg") {
    return { decision, outcome: "stayed_on_solar" };
  }

  for (const [index, probe] of probes.entries()) {
    console.error(`Verifying against probe ${index + 1}/${probes.length}...`);
    const verification = await verify({
      original_reason: decision.reason,
      soc: probe.soc,
      usable_capacity_ah: probe.usable_capacity_ah,
      verified_discharge_a: probe.verified_discharge_a,
      required_hours: probe.required_hours,
    });
    console.error("Verification:", verification);

    if (verification.verified) {
      return { decision, verification, outcome: "confirmed_sbg", probesUsed: index + 1 };
    }

    console.error("Not verified — re-deciding from scratch with real data...");
    decision = await decide({
      soc: probe.soc,
      discharge_a: probe.verified_discharge_a,
      usable_capacity_ah: probe.usable_capacity_ah,
      pv_w: probe.pv_w ?? null,
      load_w: probe.load_w ?? null,
      mode: 1,
      hour: probe.hour,
      required_hours: probe.required_hours,
    });
    console.error("Re-decision:", decision);

    if (decision.mode !== "sbg") {
      return { decision, outcome: "reverted_to_solar", probesUsed: index + 1 };
    }
  }

  return { decision, outcome: "ran_out_of_probes_unconfirmed", probesUsed: probes.length };
}

if (isMainModule(import.meta.url)) {
  readStdin()
    .then((raw) => runLoop(JSON.parse(raw)))
    .then((result) => process.stdout.write(JSON.stringify(result, null, 2)))
    .catch((error) => {
      console.error(error);
      process.exit(1);
    });
}

import { GoogleGenAI } from "@google/genai";
import SunCalc from "suncalc";
import { SYSTEM_PROMPT } from "./system.js";

// Tried in order. When one is overloaded or failing, the next one gets the
// same request; each has its own free-tier limits. Groq's 120B goes first: it
// answers in a few seconds, while Google's free tier often answers 503 "high
// demand". Groq's 20B follows Gemini Flash Lite; it's fast but follows the
// prompt's rules less closely. Models whose provider key isn't set are skipped. Gemma is last: it
// keeps answering when Gemini is busy, but its reasons read worse. Gemini 2.5
// Flash and 2.5 Flash Lite aren't offered to new API keys.
const MODELS = [
  { provider: "groq", model: "openai/gpt-oss-120b" },
  { provider: "gemini", model: "gemini-3.5-flash-lite" },
  { provider: "gemini", model: "gemini-3.1-flash-lite" },
  { provider: "groq", model: "openai/gpt-oss-20b" },
  { provider: "gemini", model: "gemini-3.8-flash" },
  { provider: "gemini", model: "gemini-3.7-flash" },
  { provider: "gemini", model: "gemini-3.6-flash" },
  { provider: "gemini", model: "gemini-3.5-flash" },
  { provider: "gemini", model: "gemma-4-26b-a4b-it" },
];
const ATTEMPT_TIMEOUT_MS = 12_000;
const LAST_MODEL_RESERVE_MS = 20_000;
const MIN_ATTEMPT_MS = 3_000;
const TOTAL_BUDGET_MS = 50_000;
// A bad key or a rejected request won't get better on the same provider's
// next model, so that provider is skipped for the rest of the call.
const PROVIDER_FATAL_STATUSES = new Set([400, 401, 403]);
const GROQ_URL = "https://api.groq.com/openai/v1/chat/completions";

// SunCalc picks the day whose solar noon is nearest, so just after midnight
// it still returns yesterday. Asking at local noon pins the calendar day.
export function sunInfo(lat, lon, date) {
  const noon = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 12);
  const nextNoon = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1, 12);
  const today = SunCalc.getTimes(noon, lat, lon);
  const nextDay = SunCalc.getTimes(nextNoon, lat, lon);
  const altitudeDeg = (SunCalc.getPosition(date, lat, lon).altitude * 180) / Math.PI;
  return {
    today: { sunrise: today.sunrise.toISOString(), sunset: today.sunset.toISOString() },
    tomorrow: { sunrise: nextDay.sunrise.toISOString() },
    altitude_deg: altitudeDeg,
  };
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

// How long a model may take: early models get a short slot and always leave
// the reserve for the last one; the last one gets whatever is left.
function attemptTimeout(isLast, deadline) {
  const remaining = deadline - Date.now();
  return isLast ? remaining : Math.min(ATTEMPT_TIMEOUT_MS, remaining - LAST_MODEL_RESERVE_MS);
}

async function askGemini(apiKey, model, { schema, prompt }, signal) {
  const ai = new GoogleGenAI({ apiKey });
  const response = await ai.models.generateContent({
    model,
    contents: prompt,
    config: {
      systemInstruction: SYSTEM_PROMPT,
      responseMimeType: "application/json",
      responseSchema: schema,
      abortSignal: signal,
    },
  });
  return JSON.parse(response.text);
}

// Groq speaks the OpenAI Chat Completions API; strict JSON schema output
// needs every object to forbid extra keys.
async function askGroq(apiKey, model, { schema, prompt }, signal) {
  const response = await fetch(GROQ_URL, {
    method: "POST",
    headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
    signal,
    body: JSON.stringify({
      model,
      messages: [
        { role: "system", content: SYSTEM_PROMPT },
        { role: "user", content: prompt },
      ],
      response_format: {
        type: "json_schema",
        json_schema: { name: "decision", strict: true, schema: { ...schema, additionalProperties: false } },
      },
    }),
  });
  const body = await response.json();
  if (!response.ok) {
    throw Object.assign(new Error(JSON.stringify(body.error ?? body)), { status: response.status });
  }
  return JSON.parse(body.choices[0].message.content);
}

const PROVIDERS = {
  gemini: { keyName: "GEMINI_API_KEY", ask: askGemini },
  groq: { keyName: "GROQ_API_KEY", ask: askGroq },
};

export async function generateStructured(request) {
  const available = MODELS.filter(({ provider }) => process.env[PROVIDERS[provider].keyName]);
  if (available.length === 0) {
    throw new Error("GEMINI_API_KEY is not set");
  }

  const deadline = Date.now() + TOTAL_BUDGET_MS;
  const failedProviders = new Set();
  let lastError = new Error("no model could be tried in time");
  for (const [index, { provider, model }] of available.entries()) {
    const timeout = attemptTimeout(index === available.length - 1, deadline);
    if (timeout < MIN_ATTEMPT_MS || failedProviders.has(provider)) continue;
    const { keyName, ask } = PROVIDERS[provider];
    const startedAt = Date.now();
    console.error(`Calling ${model}...`);
    try {
      const result = await ask(process.env[keyName], model, request, AbortSignal.timeout(timeout));
      console.error(`Got a response from ${model} in ${Date.now() - startedAt}ms`);
      return { result, model };
    } catch (error) {
      lastError = error;
      console.error(`${model} failed after ${Date.now() - startedAt}ms: ${error.status ?? error.name} ${String(error.message).slice(0, 200)}`);
      if (PROVIDER_FATAL_STATUSES.has(error.status)) failedProviders.add(provider);
    }
  }
  throw lastError;
}

export async function runAgent({ schema, buildPrompt }) {
  console.error("Reading input from stdin...");
  const raw = await readStdin();
  const input = JSON.parse(raw);
  console.error("Input:", input);

  const { result, model } = await generateStructured({ schema, prompt: buildPrompt(input) });
  process.stdout.write(JSON.stringify({ ...result, model }));
  process.exit(0);
}

export function isMainModule(url) {
  return url === `file://${process.argv[1]}`;
}

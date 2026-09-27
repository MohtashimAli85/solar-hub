import { GoogleGenAI } from "@google/genai";
import SunCalc from "suncalc";

const MODEL = "gemini-3.5-flash-lite";

export function sunInfo(lat, lon, date) {
  const tomorrow = new Date(date.getTime() + 24 * 60 * 60 * 1000);
  const today = SunCalc.getTimes(date, lat, lon);
  const nextDay = SunCalc.getTimes(tomorrow, lat, lon);
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

export async function generateStructured({ schema, prompt }) {
  const apiKey = process.env.GEMINI_API_KEY;
  if (!apiKey) {
    throw new Error("GEMINI_API_KEY is not set");
  }

  console.error(`Calling ${MODEL}...`);
  const startedAt = Date.now();
  const ai = new GoogleGenAI({ apiKey });
  const response = await ai.models.generateContent({
    model: MODEL,
    contents: prompt,
    config: {
      responseMimeType: "application/json",
      responseSchema: schema,
    },
  });
  console.error(`Got a response in ${Date.now() - startedAt}ms`);

  return JSON.parse(response.text);
}

export async function runAgent({ schema, buildPrompt }) {
  console.error("Reading input from stdin...");
  const raw = await readStdin();
  const input = JSON.parse(raw);
  console.error("Input:", input);

  const result = await generateStructured({ schema, prompt: buildPrompt(input) });
  process.stdout.write(JSON.stringify(result));
  process.exit(0);
}

export function isMainModule(url) {
  return url === `file://${process.argv[1]}`;
}

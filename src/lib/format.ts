import type { InverterFields } from "./types";

export function fmtNumber(value: number | null | undefined, digits = 1): string {
  if (value == null || Number.isNaN(value)) return "–";
  return value.toLocaleString("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: digits,
  });
}

export function fmtSigned(value: number | null | undefined, digits = 1): string {
  if (value == null || Number.isNaN(value)) return "–";
  const sign = value > 0 ? "+" : "";
  return `${sign}${fmtNumber(value, digits)}`;
}

/** "under a minute", "7 min", "5 h 50 min", "over a day". */
export function fmtDuration(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes < 1) return "under a minute";
  if (minutes < 60) return `${Math.round(minutes)} min`;
  if (minutes >= 24 * 60) return "over a day";
  const hours = Math.floor(minutes / 60);
  const rest = Math.round(minutes % 60);
  if (rest === 60) return `${hours + 1} h`;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

export function fmtHours(hours: number | null | undefined): string {
  if (hours == null || hours === Infinity) return "∞";
  return `${fmtNumber(hours, 1)}h`;
}

export function fmtTime(timestamp: string | null | undefined): string {
  if (!timestamp) return "–";
  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) return "–";
  return parsed.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" });
}

export function fmtDateTime(timestamp: string | null | undefined): string {
  if (!timestamp) return "–";
  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) return "–";
  return parsed.toLocaleString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "numeric",
  });
}
export function fieldValue(fields: InverterFields, names: string[]): number | null {
  for (const name of names) {
    const raw: unknown = fields[name];
    if (raw == null) return null;
    const item: unknown = Array.isArray(raw) ? (raw.length > 0 ? raw[raw.length - 1] : null) : raw;
    if (item == null) continue;
    if (typeof item === "number" && Number.isFinite(item)) return item;
    if (typeof item !== "object" || item === null) continue;
    const entry = item as Record<string, unknown>;
    const number = entry["value"];
    if (typeof number === "number" && Number.isFinite(number)) return number;
    if (typeof number === "string") {
      const parsed = parseFloat(number);
      if (Number.isFinite(parsed)) return parsed;
    }
  }
  return null;
}

export function flowPowerWatts(flow: unknown): number | null {
  if (flow == null) return null;
  if (typeof flow === "number") return Number.isFinite(flow) ? flow : null;
  if (typeof flow === "string") {
    const parsed = parseFloat(flow);
    return Number.isFinite(parsed) ? parsed : null;
  }
  if (typeof flow !== "object") return null;
  const f = flow as Record<string, unknown>;
  const nested = f["value"];
  let raw: unknown;
  let unit: unknown;
  if (typeof nested === "object" && nested !== null) {
    const inner = nested as Record<string, unknown>;
    raw = inner["value"];
    unit = inner["unit"];
  } else {
    raw = nested;
    unit = f["unit"];
  }
  const parsed = typeof raw === "number" ? raw : typeof raw === "string" ? parseFloat(raw) : NaN;
  if (!Number.isFinite(parsed)) return null;
  const unitText = typeof unit === "string" ? unit.toLowerCase() : "";
  if (unitText === "kw") return parsed * 1000;
  return unitText === "" || unitText === "w" ? parsed : null;
}

export function fieldPowerWatts(fields: InverterFields, names: string[]): number | null {
  for (const name of names) {
    const raw: unknown = fields[name];
    if (raw == null) continue;
    const item: unknown = Array.isArray(raw) ? (raw.length > 0 ? raw[raw.length - 1] : null) : raw;
    if (item == null) continue;
    if (typeof item !== "object") continue;
    const entry = item as Record<string, unknown>;
    const number = entry["value"];
    if (typeof number !== "number" && typeof number !== "string") continue;
    const parsed = typeof number === "number" ? number : parseFloat(number);
    if (!Number.isFinite(parsed)) continue;
    return entry["unit"] === "kW" ? parsed * 1000 : parsed;
  }
  return null;
}

export function fieldText(fields: InverterFields, names: string[]): string | null {
  for (const name of names) {
    const raw: unknown = fields[name];
    if (raw == null) continue;
    const item: unknown = Array.isArray(raw) ? raw[raw.length - 1] : raw;
    if (item == null) continue;
    if (typeof item === "string" && item.trim() !== "") return item;
    if (typeof item !== "object") continue;
    const entry = item as Record<string, unknown>;
    const display = entry["valueDisplay"];
    if (typeof display === "string" && display.trim() !== "") return display;
    const value = entry["value"];
    if (typeof value === "string" && value.trim() !== "") return value;
  }
  return null;
}

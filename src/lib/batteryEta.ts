import { fmtDuration } from "./format";
import type { BatterySnapshot } from "./types";

export const EMPTY_TARGET_PERCENT = 15;
const MOVING_CURRENT_A = 0.3;

export type BatteryEta =
  | { kind: "discharging"; minutes: number; at: Date }
  | { kind: "charging"; minutes: number; at: Date }
  | { kind: "below_target" }
  | { kind: "full" }
  | { kind: "steady" };

/** A charge rate this slow is just the sun right now; extrapolating it past this point is noise. */
const SLOW_CHARGE_LIMIT_MINUTES = 12 * 60;

/** Time until 15% while discharging, or until full while charging, at the current rate. */
export function batteryEta(battery: BatterySnapshot, now = Date.now()): BatteryEta {
  const { current, remaining_capacity: remaining, rated_capacity: rated } = battery;
  if (rated <= 0) return { kind: "steady" };
  if (current <= -MOVING_CURRENT_A) {
    const usable = remaining - (rated * EMPTY_TARGET_PERCENT) / 100;
    if (usable <= 0) return { kind: "below_target" };
    const minutes = (usable / -current) * 60;
    return { kind: "discharging", minutes, at: new Date(now + minutes * 60_000) };
  }
  if (current >= MOVING_CURRENT_A) {
    const missing = rated - remaining;
    if (missing <= rated * 0.005) return { kind: "full" };
    const minutes = (missing / current) * 60;
    return { kind: "charging", minutes, at: new Date(now + minutes * 60_000) };
  }
  return battery.soc >= 99 ? { kind: "full" } : { kind: "steady" };
}

function clockText(at: Date, now: number): string {
  const time = at.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  const today = new Date(now);
  const tomorrow = new Date(now + 86_400_000);
  if (at.toDateString() === today.toDateString()) return `around ${time}`;
  if (at.toDateString() === tomorrow.toDateString()) return `around ${time} tomorrow`;
  return `around ${at.toLocaleDateString("en-US", { weekday: "short" })} ${time}`;
}

/** Charging can't continue past sunset; without a sunset, a very slow rate is treated the same way. */
function fillsInTime(eta: Extract<BatteryEta, { kind: "charging" }>, sunset: number | null): boolean {
  return sunset != null ? eta.at.getTime() <= sunset : eta.minutes <= SLOW_CHARGE_LIMIT_MINUTES;
}

export function etaDuration(eta: BatteryEta, sunset: number | null = null): string {
  switch (eta.kind) {
    case "charging":
      return fillsInTime(eta, sunset) ? `~${fmtDuration(eta.minutes)}` : "Not today at this rate";
    case "discharging":
      return eta.minutes >= 24 * 60 ? "over a day" : `~${fmtDuration(eta.minutes)}`;
    case "below_target":
      return `Below ${EMPTY_TARGET_PERCENT}%`;
    case "full":
      return "Full";
    case "steady":
      return "–";
  }
}

/** The planner's day: when today's forecast sun (after the planned loads) fills the battery, or how full it gets by sunset. */
export interface SunOutlook {
  fullBy: string | null;
  sunsetSoc: number | null;
}

function byForecast(outlook: SunOutlook | null, now: number): { headline: string; detail: string } | null {
  if (!outlook) return null;
  if (outlook.fullBy) {
    const at = new Date(outlook.fullBy);
    if (at.getTime() > now) {
      return { headline: `Full ${clockText(at, now)}`, detail: "on today's sun forecast, after the planned loads" };
    }
  }
  if (outlook.sunsetSoc != null) {
    return { headline: `About ${Math.round(outlook.sunsetSoc)}% by sunset`, detail: "on today's sun forecast, after the planned loads" };
  }
  return null;
}

export function describeEta(
  eta: BatteryEta,
  now = Date.now(),
  sunset: number | null = null,
  outlook: SunOutlook | null = null,
): { headline: string; detail: string | null } {
  if (eta.kind === "charging") {
    const planned = byForecast(outlook, now);
    if (planned) return planned;
  }
  switch (eta.kind) {
    case "discharging":
      return {
        headline: `Down to ${EMPTY_TARGET_PERCENT}% in ${eta.minutes >= 24 * 60 ? "over a day" : `~${fmtDuration(eta.minutes)}`}`,
        detail: eta.minutes >= 24 * 60 ? "at the current draw" : `${clockText(eta.at, now)} · at the current draw`,
      };
    case "charging":
      if (!fillsInTime(eta, sunset)) {
        return {
          headline: "Charging slowly for now",
          detail: "the sun is still low — it speeds up as the day goes on",
        };
      }
      return {
        headline: `Full in ~${fmtDuration(eta.minutes)}`,
        detail: `${clockText(eta.at, now)} · at the current charge rate`,
      };
    case "below_target":
      return { headline: `Below ${EMPTY_TARGET_PERCENT}%`, detail: "the inverter may cut off soon" };
    case "full":
      return { headline: "Full", detail: null };
    case "steady":
      return { headline: "Holding steady", detail: "barely any current in or out" };
  }
}

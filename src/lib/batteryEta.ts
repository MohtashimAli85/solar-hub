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

export function etaDuration(eta: BatteryEta): string {
  switch (eta.kind) {
    case "discharging":
    case "charging":
      return eta.minutes >= 24 * 60 ? "over a day" : `~${fmtDuration(eta.minutes)}`;
    case "below_target":
      return `Below ${EMPTY_TARGET_PERCENT}%`;
    case "full":
      return "Full";
    case "steady":
      return "–";
  }
}

export function describeEta(eta: BatteryEta, now = Date.now()): { headline: string; detail: string | null } {
  switch (eta.kind) {
    case "discharging":
      return {
        headline: `${EMPTY_TARGET_PERCENT}% in ${eta.minutes >= 24 * 60 ? "over a day" : `~${fmtDuration(eta.minutes)}`}`,
        detail: eta.minutes >= 24 * 60 ? "at the current draw" : `${clockText(eta.at, now)} · at the current draw`,
      };
    case "charging":
      return {
        headline: `Full in ${eta.minutes >= 24 * 60 ? "over a day" : `~${fmtDuration(eta.minutes)}`}`,
        detail: eta.minutes >= 24 * 60 ? "at the current charge rate" : `${clockText(eta.at, now)} · at the current charge rate`,
      };
    case "below_target":
      return { headline: `Below ${EMPTY_TARGET_PERCENT}%`, detail: "the inverter may cut off soon" };
    case "full":
      return { headline: "Full", detail: null };
    case "steady":
      return { headline: "Holding steady", detail: "barely any current in or out" };
  }
}

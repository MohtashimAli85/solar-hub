export const IDLE_CURRENT_THRESHOLD_A = 0.05;

export interface ProtectionFlag {
  bit: number;
  label: string;
}

export const PROTECTION_FLAGS: ProtectionFlag[] = [
  { bit: 0, label: "Cell overvoltage" },
  { bit: 1, label: "Cell undervoltage" },
  { bit: 2, label: "Pack overvoltage" },
  { bit: 3, label: "Pack undervoltage" },
  { bit: 4, label: "Temp sensor error" },
  { bit: 5, label: "Charge over-temp" },
  { bit: 6, label: "Discharge over-temp" },
  { bit: 7, label: "Charge under-temp" },
  { bit: 8, label: "Discharge under-temp" },
  { bit: 9, label: "Overcurrent" },
  { bit: 10, label: "Pack overcurrent" },
  { bit: 11, label: "Short circuit" },
  { bit: 12, label: "AFE error" },
];

export interface FetStatus {
  charge: boolean;
  discharge: boolean;
}

export function parseFetStatus(fetStatus: number | null | undefined): FetStatus {
  const value = fetStatus ?? 0;
  return {
    charge: (value & 0x01) !== 0,
    discharge: (value & 0x02) !== 0,
  };
}

export function parseProtectionFlags(protectionStatus: number): ProtectionFlag[] {
  const flags: ProtectionFlag[] = [];
  for (const flag of PROTECTION_FLAGS) {
    if ((protectionStatus & (1 << flag.bit)) !== 0) {
      flags.push(flag);
    }
  }
  for (let bit = 13; bit < 16; bit += 1) {
    if ((protectionStatus & (1 << bit)) !== 0) {
      flags.push({ bit, label: `Unknown flag (bit ${bit})` });
    }
  }
  return flags;
}

export function parseBalanceStatus(balanceStatus: number): { count: number; cells: number[] } {
  const cells: number[] = [];
  for (let bit = 0; bit < 16; bit += 1) {
    if ((balanceStatus & (1 << bit)) !== 0) {
      cells.push(bit + 1);
    }
  }
  return { count: cells.length, cells };
}

export type Severity = "success" | "warning" | "destructive";

export function socSeverity(soc: number): Severity {
  if (soc < 20) return "destructive";
  if (soc < 50) return "warning";
  return "success";
}

export function cellDeltaSeverity(delta: number): Severity {
  if (delta < 0.05) return "success";
  if (delta < 0.1) return "warning";
  return "destructive";
}

export function temperatureSeverity(temperature: number): Severity {
  if (temperature >= 50 || temperature <= 0) return "destructive";
  if (temperature >= 42 || temperature <= 5) return "warning";
  return "success";
}

export type ChargeState = "charging" | "discharging" | "idle";

export function chargeState(current: number): ChargeState {
  if (Math.abs(current) < IDLE_CURRENT_THRESHOLD_A) return "idle";
  return current < 0 ? "discharging" : "charging";
}
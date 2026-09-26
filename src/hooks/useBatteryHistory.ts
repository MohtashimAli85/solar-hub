import { useEffect, useRef, useState } from "react";
import type { BatterySnapshot } from "@/lib/types";

export interface BatteryHistoryPoint {
  t: number;
  soc: number;
  voltage: number;
  current: number;
}

const MAX_POINTS = 600;
const MIN_SAMPLE_GAP_MS = 2000;

export function useBatteryHistory(snapshot: BatterySnapshot | null): BatteryHistoryPoint[] {
  const [history, setHistory] = useState<BatteryHistoryPoint[]>([]);
  const lastSampleAt = useRef(0);
  const lastDeviceId = useRef<string | null>(null);

  useEffect(() => {
    if (!snapshot) return;
    if (snapshot.device_id !== lastDeviceId.current) {
      lastDeviceId.current = snapshot.device_id;
      lastSampleAt.current = 0;
      setHistory([]);
      return;
    }
    const now = Date.now();
    if (now - lastSampleAt.current < MIN_SAMPLE_GAP_MS) return;
    lastSampleAt.current = now;
    const point: BatteryHistoryPoint = {
      t: now,
      soc: snapshot.soc,
      voltage: snapshot.voltage,
      current: snapshot.current,
    };
    setHistory((previous) => {
      const next = [...previous, point];
      return next.length > MAX_POINTS ? next.slice(next.length - MAX_POINTS) : next;
    });
  }, [snapshot]);

  return history;
}
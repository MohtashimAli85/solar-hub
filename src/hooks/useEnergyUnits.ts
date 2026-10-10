import { listen, type UnlistenFn } from "@/lib/transport";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  assignMeterRange,
  getEnergySummary,
  removeMeterAssignment,
  removeMeterReading,
  saveLastBillUnits,
  saveMeterReadings,
  setActiveMeter,
  setBillReading,
  setStandbyWatts,
} from "@/lib/tauri";
import type { EnergySummary, MeterNumber } from "@/lib/types";

const SUMMARY_KEY = ["energy", "summary"] as const;

export function useEnergyUnits() {
  const queryClient = useQueryClient();

  const summaryQuery = useQuery({
    queryKey: SUMMARY_KEY,
    queryFn: getEnergySummary,
    refetchInterval: 5 * 60_000,
  });

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<EnergySummary>("energy://updated", (event) => {
      queryClient.setQueryData(SUMMARY_KEY, event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [queryClient]);

  const store = (summary: EnergySummary) => queryClient.setQueryData(SUMMARY_KEY, summary);

  const saveReading = useMutation({
    mutationFn: ({ day, time }: { day: number; time: string }) => setBillReading(day, time),
    onSuccess: store,
  });

  const saveStandby = useMutation({
    mutationFn: (watts: number) => setStandbyWatts(watts),
    onSuccess: store,
  });

  const switchMeter = useMutation({
    mutationFn: ({ meter, at }: { meter: MeterNumber; at: string | null }) => setActiveMeter(meter, at),
    onSuccess: store,
  });

  const assignRange = useMutation({
    mutationFn: ({ from, to, meter }: { from: string; to: string; meter: MeterNumber }) => assignMeterRange(from, to, meter),
    onSuccess: store,
  });

  const removeAssignment = useMutation({
    mutationFn: (from: string) => removeMeterAssignment(from),
    onSuccess: store,
  });

  const saveReadings = useMutation({
    mutationFn: ({
      baseAt,
      base1,
      base2,
      at,
      meter1,
      meter2,
    }: {
      baseAt: string | null;
      base1: number;
      base2: number;
      at: string | null;
      meter1: number;
      meter2: number;
    }) => saveMeterReadings(baseAt, base1, base2, at, meter1, meter2),
    onSuccess: store,
  });

  const saveBillUnits = useMutation({
    mutationFn: ({ meter1, meter2 }: { meter1: number; meter2: number }) => saveLastBillUnits(meter1, meter2),
    onSuccess: store,
  });

  const removeReading = useMutation({
    mutationFn: (at: string) => removeMeterReading(at),
    onSuccess: store,
  });

  return {
    summary: summaryQuery.data,
    isLoading: summaryQuery.isLoading,
    saveReading,
    saveStandby,
    switchMeter,
    assignRange,
    removeAssignment,
    saveReadings,
    removeReading,
    saveBillUnits,
  };
}

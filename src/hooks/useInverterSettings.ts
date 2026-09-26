import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  getInverterSettings,
  setAcInputRange,
  setChargerPriority,
  setGridFeedIn,
  setHighCutoffVoltage,
  setLowBatteryCutoffVoltage,
  setLowDcCutoffSoc,
  setMaxTotalChargeCurrent,
  setMaxUtilityChargeCurrent,
  setOutputPriority,
  setSmartLoad as writeSmartLoad,
} from "@/lib/tauri";
import type { InverterSettings } from "@/lib/types";
import { SNAPSHOT_KEY } from "@/hooks/useInverterSnapshot";

const SETTINGS_KEY = ["inverter", "settings"] as const;

function useSettingsMutation<TArg>(mutationFn: (arg: TArg) => Promise<InverterSettings>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: (settings) => {
      queryClient.setQueryData(SETTINGS_KEY, settings);
      queryClient.invalidateQueries({ queryKey: SNAPSHOT_KEY });
    },
  });
}

export function useInverterSettings() {
  const settingsQuery = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: getInverterSettings,
    refetchInterval: 30_000,
    retry: 1,
  });

  const refresh = () => {
    settingsQuery.refetch();
  };

  const setPriority = useSettingsMutation(setOutputPriority);
  const setCharger = useSettingsMutation(setChargerPriority);
  const setAcInput = useSettingsMutation(setAcInputRange);
  const setFeedIn = useSettingsMutation(setGridFeedIn);
  const setSmartLoad = useSettingsMutation(writeSmartLoad);
  const setLowCutoff = useSettingsMutation(setLowBatteryCutoffVoltage);
  const setHighCutoff = useSettingsMutation(setHighCutoffVoltage);
  const setLowSoc = useSettingsMutation(setLowDcCutoffSoc);
  const setMaxCharge = useSettingsMutation(setMaxTotalChargeCurrent);
  const setMaxUtilityCharge = useSettingsMutation(setMaxUtilityChargeCurrent);

  return {
    settings: settingsQuery.data,
    isLoading: settingsQuery.isLoading,
    isFetching: settingsQuery.isFetching,
    error: settingsQuery.error ? String(settingsQuery.error) : null,
    refresh,
    setPriority,
    setCharger,
    setAcInput,
    setFeedIn,
    setSmartLoad,
    setLowCutoff,
    setHighCutoff,
    setLowSoc,
    setMaxCharge,
    setMaxUtilityCharge,
  };
}
import { useQuery } from "@tanstack/react-query";
import { getDeviceDetails } from "@/lib/tauri";

const DEVICE_DETAILS_KEY = ["inverter", "deviceDetails"] as const;

export function useInverterDeviceDetails() {
  const deviceDetailsQuery = useQuery({
    queryKey: DEVICE_DETAILS_KEY,
    queryFn: getDeviceDetails,
    staleTime: 30 * 60 * 1000,
    refetchInterval: 30 * 60 * 1000,
    retry: 1,
  });

  const refresh = () => {
    deviceDetailsQuery.refetch();
  };

  return {
    deviceDetails: deviceDetailsQuery.data ?? null,
    isLoading: deviceDetailsQuery.isLoading,
    isFetching: deviceDetailsQuery.isFetching,
    error: deviceDetailsQuery.error ? String(deviceDetailsQuery.error) : null,
    refresh,
  };
}
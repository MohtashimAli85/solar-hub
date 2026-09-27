import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { getInverterSnapshot } from "@/lib/tauri";
import type { InverterSnapshot } from "@/lib/types";

export const SNAPSHOT_KEY = ["inverter", "snapshot"] as const;

export function useInverterSnapshot() {
  const queryClient = useQueryClient();

  const snapshotQuery = useQuery({
    queryKey: SNAPSHOT_KEY,
    queryFn: getInverterSnapshot,
    refetchInterval: 30_000,
    retry: 1,
  });

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<InverterSnapshot>("inverter://snapshot", (event) => {
      queryClient.setQueryData(SNAPSHOT_KEY, event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [queryClient]);

  const refresh = () => {
    snapshotQuery.refetch();
  };

  return {
    snapshot: snapshotQuery.data,
    updatedAt: snapshotQuery.dataUpdatedAt || null,
    isLoading: snapshotQuery.isLoading,
    isFetching: snapshotQuery.isFetching,
    error: snapshotQuery.error ? String(snapshotQuery.error) : null,
    refresh,
  };
}
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRemoteStatus, regenerateRemotePin, setRemoteEnabled } from "@/lib/tauri";
import type { RemoteStatus } from "@/lib/types";

const REMOTE_KEY = ["remote", "status"] as const;

export function useRemoteAccess() {
  const queryClient = useQueryClient();
  const statusQuery = useQuery({
    queryKey: REMOTE_KEY,
    queryFn: getRemoteStatus,
    refetchInterval: 15_000,
  });
  const onSuccess = (status: RemoteStatus) => queryClient.setQueryData(REMOTE_KEY, status);
  const toggle = useMutation({ mutationFn: setRemoteEnabled, onSuccess });
  const regeneratePin = useMutation({ mutationFn: regenerateRemotePin, onSuccess });

  return {
    status: statusQuery.data ?? null,
    error: statusQuery.error ? String(statusQuery.error) : null,
    toggle,
    regeneratePin,
  };
}

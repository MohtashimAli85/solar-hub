import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  dismissAutomationWarning,
  forceAutomationCheck,
  getAutomationConfig,
  getAutomationStatus,
  sendTestNotification,
  updateAutomationConfig,
} from "@/lib/tauri";
import type { AutomationConfig, AutomationStatus } from "@/lib/types";

const STATUS_KEY = ["automation", "status"] as const;
const CONFIG_KEY = ["automation", "config"] as const;

export function useAutomation() {
  const queryClient = useQueryClient();

  const statusQuery = useQuery({
    queryKey: STATUS_KEY,
    queryFn: getAutomationStatus,
    refetchInterval: 10_000,
  });

  const configQuery = useQuery({
    queryKey: CONFIG_KEY,
    queryFn: getAutomationConfig,
  });

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<AutomationStatus>("automation://status", (event) => {
      queryClient.setQueryData(STATUS_KEY, event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [queryClient]);

  const saveConfig = useMutation({
    mutationFn: (config: AutomationConfig) => updateAutomationConfig(config),
    onSuccess: (config) => {
      queryClient.setQueryData(CONFIG_KEY, config);
      queryClient.setQueryData(STATUS_KEY, (previous: AutomationStatus | undefined) =>
        previous ? { ...previous, enabled: config.enabled } : previous,
      );
    },
  });

  const checkNow = useMutation({
    mutationFn: forceAutomationCheck,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });

  const dismissWarning = useMutation({
    mutationFn: dismissAutomationWarning,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });

  const testNotification = useMutation({
    mutationFn: sendTestNotification,
  });

  return {
    status: statusQuery.data,
    config: configQuery.data,
    isLoading: statusQuery.isLoading || configQuery.isLoading,
    saveConfig,
    checkNow,
    dismissWarning,
    testNotification,
  };
}
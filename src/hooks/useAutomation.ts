import { listen, type UnlistenFn } from "@/lib/transport";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  forceAutomationCheck,
  getAutomationConfig,
  getAutomationDecisions,
  getAutomationInsights,
  getAutomationStatus,
  openRecordsFolder,
  sendTestNotification,
  updateAutomationConfig,
} from "@/lib/tauri";
import type { AutomationConfig, AutomationStatus } from "@/lib/types";

const STATUS_KEY = ["automation", "status"] as const;
const CONFIG_KEY = ["automation", "config"] as const;
const INSIGHTS_KEY = ["automation", "insights"] as const;
const DECISIONS_KEY = ["automation", "decisions"] as const;

const CHECK_TIMEOUT_MS = 90_000;

export function useAutomation({ withInsights = false, decisionLimit = 0 } = {}) {
  const queryClient = useQueryClient();
  const [checkRequestedAt, setCheckRequestedAt] = useState<number | null>(null);
  const [lastStatusEventAt, setLastStatusEventAt] = useState(0);

  const statusQuery = useQuery({
    queryKey: STATUS_KEY,
    queryFn: getAutomationStatus,
    refetchInterval: 10_000,
  });

  const configQuery = useQuery({
    queryKey: CONFIG_KEY,
    queryFn: getAutomationConfig,
  });

  const insightsQuery = useQuery({
    queryKey: INSIGHTS_KEY,
    queryFn: getAutomationInsights,
    refetchInterval: 30_000,
    enabled: withInsights,
  });

  const decisionsQuery = useQuery({
    queryKey: [...DECISIONS_KEY, decisionLimit],
    queryFn: () => getAutomationDecisions(decisionLimit),
    refetchInterval: 60_000,
    enabled: decisionLimit > 0,
  });

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<AutomationStatus>("automation://status", (event) => {
      setLastStatusEventAt(Date.now());
      queryClient.setQueryData(STATUS_KEY, event.payload);
      queryClient.invalidateQueries({ queryKey: INSIGHTS_KEY });
      queryClient.invalidateQueries({ queryKey: DECISIONS_KEY });
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
        previous ? { ...previous, enabled: config.enabled, dry_run: config.dry_run } : previous,
      );
    },
  });

  const checkNow = useMutation({
    mutationFn: forceAutomationCheck,
    onMutate: () => setCheckRequestedAt(Date.now()),
    onError: () => setCheckRequestedAt(null),
  });

  useEffect(() => {
    if (checkRequestedAt == null) return;
    const id = window.setTimeout(() => setCheckRequestedAt(null), CHECK_TIMEOUT_MS);
    return () => window.clearTimeout(id);
  }, [checkRequestedAt]);

  const checking = checkNow.isPending || (checkRequestedAt != null && lastStatusEventAt < checkRequestedAt);
  const testNotification = useMutation({ mutationFn: sendTestNotification });
  const openFolder = useMutation({ mutationFn: openRecordsFolder });

  return {
    status: statusQuery.data,
    config: configQuery.data,
    insights: insightsQuery.data,
    decisions: decisionsQuery.data,
    isLoading: statusQuery.isLoading || configQuery.isLoading,
    saveConfig,
    checkNow,
    checking,
    testNotification,
    openFolder,
  };
}

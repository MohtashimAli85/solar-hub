import { listen, type UnlistenFn } from "@/lib/transport";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  answerLoadsQuestion,
  hotplateDone,
  editLoad,
  forceAutomationCheck,
  refreshForecast,
  getAutomationConfig,
  getAutomationDecisions,
  getAutomationInsights,
  getAutomationStatus,
  markGeyserDone,
  switchGeyser,
  toggleGeyserElement,
  markWaterDone,
  openRecordsFolder,
  readScooterNow,
  sendTestNotification,
  skipHotplateToday,
  skipScooterToday,
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
  const [forecastRequestedAt, setForecastRequestedAt] = useState<number | null>(null);

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

  const forecastRefresh = useMutation({
    mutationFn: refreshForecast,
    onMutate: () => setForecastRequestedAt(Date.now()),
    onError: () => setForecastRequestedAt(null),
  });

  useEffect(() => {
    if (forecastRequestedAt == null) return;
    const id = window.setTimeout(() => setForecastRequestedAt(null), CHECK_TIMEOUT_MS);
    return () => window.clearTimeout(id);
  }, [forecastRequestedAt]);

  const refreshingForecast = forecastRefresh.isPending || (forecastRequestedAt != null && lastStatusEventAt < forecastRequestedAt);
  const testNotification = useMutation({ mutationFn: sendTestNotification });
  const openFolder = useMutation({ mutationFn: openRecordsFolder });
  const waterDone = useMutation({
    mutationFn: markWaterDone,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const geyserDone = useMutation({
    mutationFn: markGeyserDone,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const geyserSwitch = useMutation({
    mutationFn: switchGeyser,
    onSettled: () => queryClient.invalidateQueries({ queryKey: STATUS_KEY }),
  });
  const geyserToggle = useMutation({
    mutationFn: toggleGeyserElement,
    onSettled: () => queryClient.invalidateQueries({ queryKey: STATUS_KEY }),
  });
  const scooterSkip = useMutation({
    mutationFn: skipScooterToday,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const loadEdit = useMutation({
    mutationFn: editLoad,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const questionAnswer = useMutation({
    mutationFn: (yes: boolean) => answerLoadsQuestion(yes),
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const hotplateFinish = useMutation({
    mutationFn: hotplateDone,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const hotplateSkip = useMutation({
    mutationFn: skipHotplateToday,
    onSuccess: (status) => queryClient.setQueryData(STATUS_KEY, status),
  });
  const scooterRead = useMutation({
    mutationFn: (powerCycle: boolean) => readScooterNow(powerCycle),
    onSettled: () => queryClient.invalidateQueries({ queryKey: STATUS_KEY }),
  });

  return {
    status: statusQuery.data,
    config: configQuery.data,
    insights: insightsQuery.data,
    decisions: decisionsQuery.data,
    isLoading: statusQuery.isLoading || configQuery.isLoading,
    saveConfig,
    checkNow,
    checking,
    forecastRefresh,
    refreshingForecast,
    testNotification,
    openFolder,
    waterDone,
    geyserDone,
    geyserSwitch,
    geyserToggle,
    scooterRead,
    scooterSkip,
    questionAnswer,
    loadEdit,
    hotplateFinish,
    hotplateSkip,
  };
}

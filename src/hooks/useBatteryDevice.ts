import { listen, type UnlistenFn } from "@/lib/transport";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import {
  connectBmsDevice,
  disconnectBmsDevice,
  getBatteryConnection,
  getBatteryState,
  reconnectSavedBms,
  scanBmsDevices,
} from "@/lib/tauri";
import type { BatterySnapshot, ConnectionStatus, DiscoveredDevice } from "@/lib/types";

const SNAPSHOT_KEY = ["battery", "snapshot"] as const;
const CONNECTION_KEY = ["battery", "connection"] as const;

export function useBatteryDevice() {
  const queryClient = useQueryClient();
  const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
  const [scanning, setScanning] = useState(false);
  const [busyDeviceId, setBusyDeviceId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const snapshotQuery = useQuery({
    queryKey: SNAPSHOT_KEY,
    queryFn: async () => (await getBatteryState()) ?? null,
  });

  const connectionQuery = useQuery({
    queryKey: CONNECTION_KEY,
    queryFn: getBatteryConnection,
    refetchInterval: 30_000,
  });

  useEffect(() => {
    let unlistenSnapshot: UnlistenFn | undefined;
    let unlistenConnection: UnlistenFn | undefined;
    Promise.all([
      listen<BatterySnapshot>("battery://snapshot", (event) => queryClient.setQueryData(SNAPSHOT_KEY, event.payload)),
      listen<ConnectionStatus>("battery://connection", (event) => {
        queryClient.setQueryData(CONNECTION_KEY, event.payload);
        if (event.payload.reconnecting && !event.payload.connected) {
          setNotice("Reconnecting to BMS…");
        } else {
          setNotice(
            event.payload.connected
              ? `Connected to ${event.payload.device_name ?? "BMS"}`
              : "Disconnected from BMS",
          );
        }
      }),
    ]).then(([a, b]) => {
      unlistenSnapshot = a;
      unlistenConnection = b;
    });
    return () => {
      unlistenSnapshot?.();
      unlistenConnection?.();
    };
  }, [queryClient]);

  const scan = useCallback(async () => {
    setError(null);
    setScanning(true);
    try {
      setDevices(await scanBmsDevices());
    } catch (caught) {
      setError(String(caught));
    } finally {
      setScanning(false);
    }
  }, []);

  const connect = useCallback(
    async (deviceId: string) => {
      setError(null);
      setBusyDeviceId(deviceId);
      try {
        await connectBmsDevice(deviceId);
      } catch (caught) {
        setError(String(caught));
      } finally {
        setBusyDeviceId(null);
      }
    },
    [],
  );

  const disconnect = useCallback(async () => {
    setError(null);
    try {
      await disconnectBmsDevice();
    } catch (caught) {
      setError(String(caught));
    }
  }, []);

  const reconnectSaved = useCallback(async () => {
    setError(null);
    setScanning(true);
    try {
      await reconnectSavedBms();
    } catch (caught) {
      setError(String(caught));
    } finally {
      setScanning(false);
    }
  }, []);

  return {
    snapshot: snapshotQuery.data ?? null,
    connection: connectionQuery.data ?? null,
    devices,
    scanning,
    busyDeviceId,
    error,
    notice,
    setNotice,
    isLoading: snapshotQuery.isLoading || connectionQuery.isLoading,
    scan,
    connect,
    disconnect,
    reconnectSaved,
  };
}
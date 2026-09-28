import { listen, type UnlistenFn } from "@/lib/transport";
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

export function useBatteryDevice() {
  const [snapshot, setSnapshot] = useState<BatterySnapshot | null>(null);
  const [connection, setConnection] = useState<ConnectionStatus | null>(null);
  const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
  const [scanning, setScanning] = useState(false);
  const [busyDeviceId, setBusyDeviceId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const refreshSnapshot = useCallback(async () => {
    setSnapshot(await getBatteryState());
  }, []);

  const refreshConnection = useCallback(async () => {
    setConnection(await getBatteryConnection());
  }, []);

  useEffect(() => {
    let unlistenSnapshot: UnlistenFn | undefined;
    let unlistenConnection: UnlistenFn | undefined;
    Promise.all([
      listen<BatterySnapshot>("battery://snapshot", (event) => setSnapshot(event.payload)),
      listen<ConnectionStatus>("battery://connection", (event) => {
        setConnection(event.payload);
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
    refreshSnapshot();
    refreshConnection();
    Promise.allSettled([refreshSnapshot(), refreshConnection()]).then(() => {
      setIsLoading(false);
    });

    const refreshTimer = setInterval(() => {
      refreshConnection();
    }, 30_000);
    return () => {
      clearInterval(refreshTimer);
      unlistenSnapshot?.();
      unlistenConnection?.();
    };
  }, [refreshSnapshot, refreshConnection]);

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
    snapshot,
    connection,
    devices,
    scanning,
    busyDeviceId,
    error,
    notice,
    setNotice,
    isLoading,
    scan,
    connect,
    disconnect,
    reconnectSaved,
    refreshSnapshot,
    refreshConnection,
  };
}
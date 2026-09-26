import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Loader2, RefreshCw, Zap } from "lucide-react";
import type { ConnectionStatus, DiscoveredDevice } from "@/lib/types";

interface DeviceSelectorProps {
  devices: DiscoveredDevice[];
  connection: ConnectionStatus | null;
  scanning: boolean;
  busyDeviceId: string | null;
  onScan: () => void;
  onConnect: (id: string) => void;
  onDisconnect: () => void;
  onReconnectSaved: () => void;
  savedDeviceId?: string;
}

export function DeviceSelector({
  devices,
  connection,
  scanning,
  busyDeviceId,
  onScan,
  onConnect,
  onDisconnect,
  onReconnectSaved,
  savedDeviceId,
}: DeviceSelectorProps) {
  const connected = connection?.connected ?? false;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={onScan} disabled={scanning} variant="outline">
          {scanning ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          {scanning ? "Scanning…" : "Scan for BMS"}
        </Button>
        <Button onClick={onReconnectSaved} disabled={scanning || connected} variant="secondary">
          Reconnect saved
        </Button>
        {connected ? (
          <Button onClick={onDisconnect} variant="destructive">
            Disconnect
          </Button>
        ) : null}
        {connected ? (
          <Badge variant="success">
            <Zap className="size-3" />
            {connection?.device_name ?? "Connected"}
          </Badge>
        ) : null}
        {!connected && connection?.reconnecting ? (
          <Badge variant="secondary">
            <Loader2 className="size-3 animate-spin" />
            Reconnecting…
          </Badge>
        ) : null}
        {savedDeviceId ? (
          <span className="text-xs text-muted-foreground">saved: {savedDeviceId.slice(0, 8)}</span>
        ) : null}
      </div>

      <div className="grid min-h-0 flex-1 gap-1.5 overflow-y-auto rounded-md border border-border p-2">
        {devices.length === 0 ? (
          <p className="px-2 py-1 text-sm text-muted-foreground">
            No devices yet — hit "Scan for BMS". Keep the battery powered and close.
          </p>
        ) : (
          devices.map((device) => {
            const isCurrent = connected && connection?.device_id === device.id;
            const busy = busyDeviceId === device.id;
            return (
              <div
                key={device.id}
                className="flex items-center justify-between gap-2 rounded-sm px-2 py-1.5 hover:bg-accent"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">
                    {device.name || "Unnamed BMS"}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {device.id.slice(0, 17)}…{" "}
                    {device.rssi != null ? `${device.rssi} dBm` : ""}
                  </p>
                </div>
                {isCurrent ? (
                  <Badge variant="secondary">connected</Badge>
                ) : (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy || connected}
                    onClick={() => onConnect(device.id)}
                  >
                    {busy ? <Loader2 className="animate-spin" /> : "Connect"}
                  </Button>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
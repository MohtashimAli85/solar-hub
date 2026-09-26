import { BatteryTrends } from "@/components/battery/BatteryTrends";
import { CellPackGrid } from "@/components/battery/CellPackGrid";
import { DeviceSelector } from "@/components/battery/DeviceSelector";
import { PackHealthHero } from "@/components/battery/PackHealthHero";
import { TemperatureGauges } from "@/components/battery/TemperatureGauges";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/switch";
import { fmtNumber } from "@/lib/format";
import { useBatteryDevice } from "@/hooks/useBatteryDevice";
import { useBatteryHistory } from "@/hooks/useBatteryHistory";
import { getSavedBmsDevice } from "@/lib/tauri";
import type { SavedBleDevice } from "@/lib/types";
import { AlertCircle, X } from "lucide-react";
import { useEffect, useState } from "react";

export function BatteryPage() {
  const battery = useBatteryDevice();
  const [saved, setSaved] = useState<SavedBleDevice | null>(null);
  const history = useBatteryHistory(battery.snapshot);

  useEffect(() => {
    getSavedBmsDevice().then((device) => {
      if (device) setSaved(device);
    });
  }, []);

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Battery</h1>
        <p className="text-sm text-muted-foreground">
          Is the pack healthy? JBD BMS over Bluetooth — direct pack telemetry.
        </p>
      </div>

      {battery.error ? (
        <div className="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm">
          <span className="flex items-center gap-2 text-destructive">
            <AlertCircle className="size-4" /> {battery.error}
          </span>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => battery.setNotice(null)}
          >
            <X />
          </Button>
        </div>
      ) : null}
      {battery.notice ? (
        <div className="rounded-md border border-border bg-muted px-4 py-3 text-sm">
          {battery.notice}
        </div>
      ) : null}

      <DeviceSelector
        devices={battery.devices}
        connection={battery.connection}
        scanning={battery.scanning}
        busyDeviceId={battery.busyDeviceId}
        onScan={battery.scan}
        onConnect={battery.connect}
        onDisconnect={battery.disconnect}
        onReconnectSaved={battery.reconnectSaved}
        savedDeviceId={saved?.id}
      />

      {!battery.snapshot ? (
        <EmptyState message="Connect a battery to see telemetry." />
      ) : (
        <>
          <PackHealthHero snapshot={battery.snapshot} />

          <section className="rounded-xl border border-border bg-card p-5">
            <h3 className="text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
              Cell map
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {battery.snapshot.cell_voltages.length} cells · produced {battery.snapshot.production_date || "–"} ·{" "}
              {fmtNumber(battery.snapshot.remaining_capacity, 0)}/{fmtNumber(battery.snapshot.rated_capacity, 0)} Ah
            </p>
            <div className="mt-3">
              <CellPackGrid voltages={battery.snapshot.cell_voltages} />
            </div>
          </section>

          <section className="rounded-xl border border-border bg-card p-5">
            <h3 className="text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
              Temperatures
            </h3>
            <div className="mt-3">
              <TemperatureGauges temperatures={battery.snapshot.temperatures} />
            </div>
          </section>

          <BatteryTrends history={history} />
        </>
      )}
    </div>
  );
}

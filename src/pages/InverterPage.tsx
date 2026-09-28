import { AlarmBanner } from "@/components/inverter/AlarmBanner";
import { ChargerPrioritySelector } from "@/components/inverter/ChargerPrioritySelector";
import { DeviceSummaryCard } from "@/components/inverter/DeviceSummaryCard";
import { InverterHero } from "@/components/inverter/InverterHero";
import { OutputSourcePanel } from "@/components/inverter/OutputSourcePanel";
import { SettingsControls } from "@/components/inverter/SettingsControls";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/switch";
import { AlertCircle, ChevronRight, RefreshCw } from "lucide-react";
import { useBatteryDevice } from "@/hooks/useBatteryDevice";
import { useInverterDeviceDetails } from "@/hooks/useInverterDeviceDetails";
import { useInverterSettings } from "@/hooks/useInverterSettings";
import { useInverterSnapshot } from "@/hooks/useInverterSnapshot";

export function InverterPage() {
  const snapshotHook = useInverterSnapshot();
  const settingsHook = useInverterSettings();
  const deviceDetailsHook = useInverterDeviceDetails();
  const battery = useBatteryDevice();

  const settings = settingsHook.settings ?? snapshotHook.snapshot?.settings ?? null;
  const isBusy =
    snapshotHook.isLoading || settingsHook.isFetching || deviceDetailsHook.isLoading;
  const error = snapshotHook.error ?? settingsHook.error;
  const snapshot = snapshotHook.snapshot;

  const refreshAll = () => {
    snapshotHook.refresh();
    settingsHook.refresh();
    deviceDetailsHook.refresh();
  };

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-bold tracking-tight sm:text-2xl">Inverter</h1>
        <p className="text-sm text-muted-foreground">
          Live power flows, output source and the inverter's advanced settings.
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" onClick={refreshAll} disabled={isBusy}>
          <RefreshCw className={isBusy ? "animate-spin" : ""} />
          Refresh
        </Button>
        {error ? (
          <span className="flex items-center gap-1.5 text-sm text-destructive">
            <AlertCircle className="size-4" /> {error}
          </span>
        ) : null}
      </div>

      {snapshot ? <AlarmBanner alarms={snapshot.firing_alarms} /> : null}

      {!snapshot ? (
        <EmptyState message="No inverter telemetry yet. Add Solar credentials in Settings, then refresh." />
      ) : (
        <>
          <InverterHero
            snapshot={snapshot}
            battery={battery.snapshot}
            batteryDeviceName={battery.connection?.device_name ?? null}
          />

          <div className="grid items-start gap-4 lg:grid-cols-2">
            <OutputSourcePanel
              settings={settings}
              pending={settingsHook.setPriority.isPending}
              smartLoadPending={settingsHook.setSmartLoad.isPending}
              onSet={(mode) => settingsHook.setPriority.mutate(mode)}
              onSetSmartLoad={(enabled) => settingsHook.setSmartLoad.mutate(enabled)}
              onRefresh={settingsHook.refresh}
              refreshing={settingsHook.isFetching}
            />
            <DeviceSummaryCard
              details={deviceDetailsHook.deviceDetails}
              loading={deviceDetailsHook.isLoading}
              error={deviceDetailsHook.error}
              onRefresh={deviceDetailsHook.refresh}
              refreshing={deviceDetailsHook.isFetching}
            />
          </div>

          <details className="group rounded-xl border border-border bg-card">
            <summary className="flex cursor-pointer list-none items-center justify-between gap-2 p-4 sm:p-5 [&::-webkit-details-marker]:hidden">
              <div className="flex flex-col space-y-1">
                <h3 className="text-label font-semibold uppercase tracking-wide text-muted-foreground">
                  Advanced — charger + device settings
                </h3>
                <p className="text-xs text-muted-foreground">
                  Charger priority, cutoffs, charge currents and grid behaviour.
                </p>
              </div>
              <ChevronRight className="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-90" />
            </summary>
            <div className="space-y-5 p-4 pt-0 sm:p-5 sm:pt-0">
              <ChargerPrioritySelector
                settings={settings}
                pending={settingsHook.setCharger.isPending}
                onSet={(mode) => settingsHook.setCharger.mutate(mode)}
                onRefresh={settingsHook.refresh}
                refreshing={settingsHook.isFetching}
              />

              <SettingsControls
                settings={settings}
                onRefresh={settingsHook.refresh}
                refreshing={settingsHook.isFetching}
                onSetAcInput={{
                  pending: settingsHook.setAcInput.isPending,
                  onCommit: settingsHook.setAcInput.mutate,
                }}
                onSetFeedIn={{
                  pending: settingsHook.setFeedIn.isPending,
                  onCommit: settingsHook.setFeedIn.mutate,
                }}
                onSetLowCutoff={{
                  pending: settingsHook.setLowCutoff.isPending,
                  onCommit: settingsHook.setLowCutoff.mutate,
                }}
                onSetHighCutoff={{
                  pending: settingsHook.setHighCutoff.isPending,
                  onCommit: settingsHook.setHighCutoff.mutate,
                }}
                onSetLowSoc={{
                  pending: settingsHook.setLowSoc.isPending,
                  onCommit: settingsHook.setLowSoc.mutate,
                }}
                onSetMaxCharge={{
                  pending: settingsHook.setMaxCharge.isPending,
                  onCommit: settingsHook.setMaxCharge.mutate,
                }}
                onSetMaxUtilityCharge={{
                  pending: settingsHook.setMaxUtilityCharge.isPending,
                  onCommit: settingsHook.setMaxUtilityCharge.mutate,
                }}
              />
            </div>
          </details>

          <details className="group rounded-lg border border-border bg-card shadow-sm">
            <summary className="flex cursor-pointer list-none items-center justify-between gap-2 p-4 sm:p-5 [&::-webkit-details-marker]:hidden">
              <div className="flex flex-col space-y-1">
                <h3 className="text-base font-semibold leading-none tracking-tight">
                  Raw fields
                </h3>
                <p className="text-sm text-muted-foreground">
                  Every field exactly as the inverter cloud returns it.
                </p>
              </div>
              <ChevronRight className="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-90" />
            </summary>
            <div className="p-4 pt-0 sm:p-5 sm:pt-0">
              <RawFields fields={snapshot.fields} />
            </div>
          </details>
        </>
      )}
    </div>
  );
}

function RawFields({ fields }: { fields: Record<string, unknown> }) {
  const entries = Object.entries(fields);
  if (entries.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No raw fields returned.</p>
    );
  }
  return (
    <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2 lg:grid-cols-3">
      {entries.map(([key, value]) => (
        <div
          key={key}
          className="flex items-center justify-between gap-2 rounded-md bg-muted px-3 py-2 text-sm"
        >
          <span className="text-muted-foreground">{key}</span>
          <RawValue value={value} />
        </div>
      ))}
    </div>
  );
}

function RawValue({ value }: { value: unknown }) {
  if (Array.isArray(value)) {
    const last = value[value.length - 1];
    return <span className="tabular-nums">{formatValue(last)}</span>;
  }
  return <span className="tabular-nums">{formatValue(value)}</span>;
}

function formatValue(value: unknown): string {
  if (value == null) return "–";
  if (typeof value === "object") {
    const entry = value as Record<string, unknown>;
    const raw = entry["value"];
    if (raw != null) return formatValue(raw);
  }
  return String(value);
}

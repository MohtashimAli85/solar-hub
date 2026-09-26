import { automationPhaseLabel } from "@/components/automation/AutomationSettingsCard";
import { CommandHero } from "@/components/dashboard/CommandHero";
import { OutputSourcePanel } from "@/components/inverter/OutputSourcePanel";
import { fieldPowerWatts, fieldValue, flowPowerWatts, fmtNumber } from "@/lib/format";
import { useAutomation } from "@/hooks/useAutomation";
import { useBatteryDevice } from "@/hooks/useBatteryDevice";
import { useInverterSettings } from "@/hooks/useInverterSettings";
import { useInverterSnapshot } from "@/hooks/useInverterSnapshot";

export function DashboardPage() {
  const battery = useBatteryDevice();
  const inverter = useInverterSnapshot();
  const settingsHook = useInverterSettings();
  const automation = useAutomation();

  const settings = settingsHook.settings ?? inverter.snapshot?.settings ?? null;
  const fields = inverter.snapshot?.fields ?? {};
  const loadWatts = flowPowerWatts(inverter.snapshot?.load_flow) ?? fieldPowerWatts(fields, ["load_power", "loadPower", "outputActivePower", "acOutputActivePower"]);
  const gridWatts = flowPowerWatts(inverter.snapshot?.grid_flow) ?? fieldPowerWatts(fields, ["gridPower"]) ?? fieldValue(fields, ["gridPower"]);

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
        <p className="text-sm text-muted-foreground">
          Glance and control — SoC, charge rate, PV, source and smart load.
        </p>
      </div>

      <CommandHero
        battery={battery.snapshot}
        connection={battery.connection}
        inverter={inverter.snapshot ?? null}
        batteryLoading={battery.isLoading}
        inverterLoading={inverter.isLoading}
      />

      <div className="grid gap-4 lg:grid-cols-5">
        <div className="lg:col-span-3">
          <OutputSourcePanel
            settings={settings}
            pending={settingsHook.setPriority.isPending}
            smartLoadPending={settingsHook.setSmartLoad.isPending}
            onSet={(mode) => settingsHook.setPriority.mutate(mode)}
            onSetSmartLoad={(enabled) => settingsHook.setSmartLoad.mutate(enabled)}
            onRefresh={settingsHook.refresh}
            refreshing={settingsHook.isFetching}
          />
          <div className="mt-4 flex flex-wrap gap-x-5 gap-y-1 rounded-xl border border-border bg-card px-4 py-3 text-[13px] tabular-nums text-muted-foreground">
            <span>
              load <span className="font-semibold text-foreground">{fmtNumber(loadWatts != null ? Math.round(loadWatts) : null, 0)} W</span>
            </span>
            <span>
              grid <span className="font-semibold text-foreground">{fmtNumber(gridWatts != null ? Math.round(gridWatts) : null, 0)} W</span>
            </span>
            <span>
              output <span className="text-foreground">{settings?.output_source_priority ?? "–"}</span>
            </span>
            <span>
              charger <span className="text-foreground">{settings?.charger_source_priority ?? "–"}</span>
            </span>
            {automation.status ? (
              <span>
                automation <span className="text-foreground">{automationPhaseLabel(automation.status.phase)}</span>
                {automation.status.last_event ? ` · ${automation.status.last_event.message}` : ""}
              </span>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

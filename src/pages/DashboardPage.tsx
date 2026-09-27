import { AutomationSummaryCard } from "@/components/dashboard/AutomationSummaryCard";
import { CommandHero } from "@/components/dashboard/CommandHero";
import { OutputSourcePanel } from "@/components/inverter/OutputSourcePanel";
import { useAutomation } from "@/hooks/useAutomation";
import { useBatteryDevice } from "@/hooks/useBatteryDevice";
import { useInverterSettings } from "@/hooks/useInverterSettings";
import { useInverterSnapshot } from "@/hooks/useInverterSnapshot";

export function DashboardPage({ onOpenAutomation }: { onOpenAutomation: () => void }) {
  const battery = useBatteryDevice();
  const inverter = useInverterSnapshot();
  const settingsHook = useInverterSettings();
  const automation = useAutomation();

  const settings = settingsHook.settings ?? inverter.snapshot?.settings ?? null;

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
        <p className="text-sm text-muted-foreground">Where your power is coming from and going, and what the automation is doing.</p>
      </div>

      <CommandHero
        battery={battery.snapshot}
        connection={battery.connection}
        inverter={inverter.snapshot ?? null}
        inverterUpdatedAt={inverter.updatedAt}
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
        </div>
        <div className="lg:col-span-2">
          <AutomationSummaryCard status={automation.status} onOpen={onOpenAutomation} />
        </div>
      </div>
    </div>
  );
}

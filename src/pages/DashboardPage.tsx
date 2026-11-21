import { AutomationSummaryCard } from "@/components/dashboard/AutomationSummaryCard";
import { CommandHero } from "@/components/dashboard/CommandHero";
import { BillCard } from "@/components/dashboard/BillCard";
import { UnitsCard } from "@/components/dashboard/UnitsCard";
import { NowStrip } from "@/components/dashboard/NowStrip";
import { SolarLoadsCard } from "@/components/dashboard/SolarLoadsCard";
import { SunCurveCard } from "@/components/dashboard/SunCurveCard";
import { OutputSourcePanel } from "@/components/inverter/OutputSourcePanel";
import { useAutomation } from "@/hooks/useAutomation";
import { useBatteryDevice } from "@/hooks/useBatteryDevice";
import { useInverterSettings } from "@/hooks/useInverterSettings";
import { useInverterSnapshot } from "@/hooks/useInverterSnapshot";
import type { AutomationConfig } from "@/lib/types";

function geyserLevels(config: AutomationConfig | undefined): number[] {
  if (!config) return [800, 1200, 2000];
  const { geyser_low_watts: low, geyser_high_watts: high } = config;
  return [...new Set([Math.min(low, high), Math.max(low, high), low + high])];
}

export function DashboardPage({ onOpenAutomation }: { onOpenAutomation: () => void }) {
  const battery = useBatteryDevice();
  const inverter = useInverterSnapshot();
  const settingsHook = useInverterSettings();
  const automation = useAutomation({ withInsights: true });

  const settings = settingsHook.settings ?? inverter.snapshot?.settings ?? null;

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-bold tracking-tight sm:text-2xl">Dashboard</h1>
        <p className="text-sm text-muted-foreground">Where your power is coming from and going, and what the automation is doing.</p>
      </div>

      <NowStrip
        loads={automation.status?.loads}
        water={automation.status?.water}
        pending={automation.loadEdit.isPending || automation.questionAnswer.isPending}
        onEdit={(edit) => automation.loadEdit.mutate(edit)}
        onAnswer={(yes) => automation.questionAnswer.mutate(yes)}
      />

      <CommandHero
        battery={battery.snapshot}
        connection={battery.connection}
        inverter={inverter.snapshot ?? null}
        inverterUpdatedAt={inverter.updatedAt}
        sunset={automation.status?.sunset ?? null}
        outlook={
          automation.status?.loads
            ? { fullBy: automation.status.loads.full_by, sunsetSoc: automation.status.loads.projected_sunset_soc }
            : null
        }
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
            className="h-full"
          />
        </div>
        <div className="lg:col-span-2">
          <AutomationSummaryCard status={automation.status} onOpen={onOpenAutomation} />
        </div>
      </div>

      <SunCurveCard
        forecast={automation.insights?.weather?.today_pv}
        measured={automation.insights?.weather?.today_measured}
        fetchedAt={automation.insights?.weather?.fetched_at}
        refreshing={automation.refreshingForecast}
        onRefresh={() => automation.forecastRefresh.mutate()}
      />

      <SolarLoadsCard
        water={automation.status?.water}
        loads={automation.status?.loads}
        waterPending={automation.waterDone.isPending}
        geyserPending={automation.geyserDone.isPending}
        scooterPending={automation.scooterRead.isPending}
        skipPending={automation.scooterSkip.isPending}
        hotplatePending={automation.hotplateFinish.isPending || automation.hotplateSkip.isPending}
        editPending={automation.loadEdit.isPending}
        config={automation.config}
        geyserLevels={geyserLevels(automation.config)}
        onWaterDone={() => automation.waterDone.mutate()}
        onGeyserDone={() => automation.geyserDone.mutate()}
        onSwitchGeyser={(watts) => automation.geyserSwitch.mutate(watts)}
        onToggleElement={(name) => automation.geyserToggle.mutate(name)}
        togglePending={automation.geyserToggle.isPending}
        switchError={
          automation.geyserSwitch.error
            ? String(automation.geyserSwitch.error)
            : automation.geyserToggle.error
              ? String(automation.geyserToggle.error)
              : null
        }
        onReadScooter={() => automation.scooterRead.mutate(true)}
        onSkipScooter={() => automation.scooterSkip.mutate()}
        onEdit={(edit) => automation.loadEdit.mutate(edit)}
        onHotplateDone={() => automation.hotplateFinish.mutate()}
        onHotplateSkip={() => automation.hotplateSkip.mutate()}
      />

      <UnitsCard />

      <BillCard />
    </div>
  );
}

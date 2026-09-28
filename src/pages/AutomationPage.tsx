import { useState } from "react";
import { AutomationHero } from "@/components/automation/AutomationHero";
import { AutomationSettingsForm } from "@/components/automation/AutomationSettingsForm";
import { DecisionTimeline } from "@/components/automation/DecisionTimeline";
import { OutagesCard } from "@/components/automation/OutagesCard";
import { RecordsCard } from "@/components/automation/RecordsCard";
import { RoutineCard } from "@/components/automation/RoutineCard";
import { Skeleton } from "@/components/automation/shared";
import { TodayCard } from "@/components/automation/TodayCard";
import { TonightCard } from "@/components/automation/TonightCard";
import { WeatherCard } from "@/components/automation/WeatherCard";
import { useAutomation } from "@/hooks/useAutomation";

const FIRST_PAGE = 8;
const MAX_DECISIONS = 60;

export function AutomationPage({ onOpenSettings }: { onOpenSettings: () => void }) {
  const [decisionLimit, setDecisionLimit] = useState(FIRST_PAGE);
  const automation = useAutomation({ withInsights: true, decisionLimit });
  const { status, config, insights } = automation;

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-bold tracking-tight sm:text-2xl">Automation</h1>
        <p className="text-sm text-muted-foreground">
          The agent plans how much battery to use each night, and keeps it charging properly by day.
        </p>
      </div>

      {!status || !config ? (
        <div className="space-y-4">
          <Skeleton className="h-48 w-full" />
          <div className="grid gap-4 lg:grid-cols-2">
            <Skeleton className="h-64" />
            <Skeleton className="h-64" />
          </div>
        </div>
      ) : (
        <>
          <AutomationHero
            status={status}
            config={config}
            insights={insights}
            saving={automation.saveConfig.isPending}
            checking={automation.checking}
            onSave={(next) => automation.saveConfig.mutate(next)}
            onCheckNow={() => automation.checkNow.mutate()}
            onOpenSettings={onOpenSettings}
          />

          {insights?.window ? (
            <div className="grid gap-4 lg:grid-cols-2">
              {insights.window === "day" ? (
                <>
                  <TodayCard insights={insights} />
                  <TonightCard insights={insights} />
                </>
              ) : (
                <div className="lg:col-span-2">
                  <TonightCard insights={insights} />
                </div>
              )}
              <RoutineCard insights={insights} />
              <WeatherCard insights={insights} />
              <OutagesCard insights={insights} />
              <RecordsCard
                insights={insights}
                fallbackDir={status.history_dir}
                opening={automation.openFolder.isPending}
                error={automation.openFolder.error ? String(automation.openFolder.error) : null}
                onOpen={() => automation.openFolder.mutate()}
              />
            </div>
          ) : (
            <RecordsCard
              insights={insights}
              fallbackDir={status.history_dir}
              opening={automation.openFolder.isPending}
              error={automation.openFolder.error ? String(automation.openFolder.error) : null}
              onOpen={() => automation.openFolder.mutate()}
            />
          )}

          <DecisionTimeline
            decisions={automation.decisions}
            canShowMore={decisionLimit < MAX_DECISIONS && (automation.decisions?.length ?? 0) >= decisionLimit}
            onShowMore={() => setDecisionLimit((limit) => Math.min(limit * 2, MAX_DECISIONS))}
          />

          <AutomationSettingsForm
            config={config}
            saving={automation.saveConfig.isPending}
            testing={automation.testNotification.isPending}
            onSave={(next) => automation.saveConfig.mutate(next)}
            onTestNotification={() => automation.testNotification.mutate()}
          />
        </>
      )}
    </div>
  );
}

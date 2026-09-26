import { AutomationConfigForm } from "@/components/automation/AutomationConfigForm";
import { AutomationStatusCard } from "@/components/automation/AutomationStatusCard";
import { Button } from "@/components/ui/button";
import { useAutomation } from "@/hooks/useAutomation";
import { AlertCircle, X } from "lucide-react";

export function AutomationPage() {
  const automation = useAutomation();

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Automation</h1>
        <p className="text-muted-foreground">
          Night SBG switching — switches once, probes the real discharge, and holds while it
          covers the window until the target hour.
        </p>
      </div>

      {automation.status?.warning ? (
        <div className="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm">
          <span className="flex items-center gap-2 text-destructive">
            <AlertCircle className="size-4" /> {automation.status.warning.message}
          </span>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => automation.dismissWarning.mutate()}
            disabled={automation.dismissWarning.isPending}
          >
            <X />
          </Button>
        </div>
      ) : null}

      <div className="grid gap-6 lg:grid-cols-2">
        <AutomationConfigForm
          config={automation.config}
          saving={automation.saveConfig.isPending}
          checking={automation.checkNow.isPending}
          testing={automation.testNotification.isPending}
          onSave={(config) => automation.saveConfig.mutate(config)}
          onCheckNow={() => automation.checkNow.mutate()}
          onTestNotification={() => automation.testNotification.mutate()}
        />
        <AutomationStatusCard status={automation.status} />
      </div>
    </div>
  );
}

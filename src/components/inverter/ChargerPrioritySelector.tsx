import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { CardRefreshHeader } from "@/components/inverter/CardRefreshHeader";
import { Loader2 } from "lucide-react";
import { CHARGER_MODES } from "@/lib/types";
import type { InverterSettings } from "@/lib/types";
import { cn } from "@/lib/utils";

interface ChargerPrioritySelectorProps {
  settings: InverterSettings | null;
  pending: boolean;
  onSet: (mode: string) => void;
  onRefresh: () => void;
  refreshing: boolean;
  disabled?: boolean;
}

export function ChargerPrioritySelector({
  settings,
  pending,
  onSet,
  onRefresh,
  refreshing,
  disabled,
}: ChargerPrioritySelectorProps) {
  const currentValue = settings?.charger_source_priority_value;
  return (
    <Card>
      <CardRefreshHeader
        title="Charger priority override"
        description="Controls how the battery charger prioritises solar vs utility."
        onRefresh={onRefresh}
        refreshing={refreshing}
      />
      <CardContent className="space-y-3">
        <div className="flex flex-wrap gap-2">
          {CHARGER_MODES.map((mode) => {
            const active = currentValue != null && String(currentValue) === mode.value;
            return (
              <Button
                key={mode.value}
                variant={active ? "default" : "outline"}
                disabled={disabled || pending}
                onClick={() => onSet(mode.value)}
              >
                {pending && "1" === mode.value ? (
                  <Loader2 className="animate-spin" />
                ) : null}
                {mode.label}
              </Button>
            );
          })}
        </div>
        {settings ? (
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <Badge variant="secondary" className={cn(pending && "opacity-60")}>
              Charger: {settings.charger_source_priority}
            </Badge>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

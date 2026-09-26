import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/switch";
import { fieldPowerWatts, fmtNumber } from "@/lib/format";
import type { InverterSnapshot } from "@/lib/types";

interface InverterSummaryCardProps {
  snapshot: InverterSnapshot | null | undefined;
  isLoading?: boolean;
}

export function InverterSummaryCard({ snapshot, isLoading = false }: InverterSummaryCardProps) {
  const fields = snapshot?.fields ?? {};
  const pv = fieldPowerWatts(fields, ["pvInputPower", "generationPower"]);
  const ac = fieldPowerWatts(fields, ["acOutputActivePower", "outputActivePower", "load_power", "loadPower"]);
  const mode = snapshot?.settings?.output_source_priority;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle>Inverter</CardTitle>
          {isLoading ? (
            <Skeleton className="h-5 w-24" />
          ) : mode ? (
            <Badge variant="outline">{mode}</Badge>
          ) : (
            <Badge variant="secondary">unconfigured</Badge>
          )}
        </div>
        <CardDescription>Solar of Things cloud</CardDescription>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="grid grid-cols-2 gap-3" aria-busy="true" aria-label="Loading inverter data">
            <div>
              <Skeleton className="h-3 w-16" />
              <Skeleton className="mt-1.5 h-6 w-20" />
            </div>
            <div>
              <Skeleton className="h-3 w-16" />
              <Skeleton className="mt-1.5 h-6 w-20" />
            </div>
            <div>
              <Skeleton className="h-3 w-16" />
              <Skeleton className="mt-1.5 h-6 w-20" />
            </div>
            <div>
              <Skeleton className="h-3 w-16" />
              <Skeleton className="mt-1.5 h-6 w-20" />
            </div>
          </div>
        ) : !snapshot ? (
          <p className="text-sm text-muted-foreground">
            No telemetry yet — add Solar credentials in Settings.
          </p>
        ) : (
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div>
              <p className="text-xs text-muted-foreground">PV input</p>
              <p className="text-lg font-semibold tabular-nums">{fmtNumber(pv, 0)} W</p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">AC output</p>
              <p className="text-lg font-semibold tabular-nums">{fmtNumber(ac, 0)} W</p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">Output priority</p>
              <p className="text-lg font-semibold">{mode ?? "–"}</p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">Charger</p>
              <p className="text-lg font-semibold">
                {snapshot.settings?.charger_source_priority ?? "–"}
              </p>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
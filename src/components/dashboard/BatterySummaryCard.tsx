import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Progress, Skeleton } from "@/components/ui/switch";
import { fmtHours, fmtNumber, fmtSigned } from "@/lib/format";
import type { BatterySnapshot, ConnectionStatus } from "@/lib/types";

interface BatterySummaryCardProps {
  snapshot: BatterySnapshot | null;
  connection: ConnectionStatus | null;
  isLoading?: boolean;
}

export function BatterySummaryCard({
  snapshot,
  connection,
  isLoading = false,
}: BatterySummaryCardProps) {
  const connected = connection?.connected ?? false;
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle>Battery</CardTitle>
          {isLoading ? (
            <Skeleton className="h-5 w-20" />
          ) : (
            <Badge variant={connected ? "success" : "secondary"}>
              {connected ? connection?.device_name ?? "Connected" : "Offline"}
            </Badge>
          )}
        </div>
        <CardDescription>JBD BMS via Bluetooth</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {isLoading ? (
          <div className="space-y-3" aria-busy="true" aria-label="Loading battery data">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <Skeleton className="h-3 w-12" />
                <Skeleton className="mt-1.5 h-6 w-20" />
              </div>
              <div>
                <Skeleton className="h-3 w-12" />
                <Skeleton className="mt-1.5 h-6 w-20" />
              </div>
            </div>
            <div className="space-y-1.5">
              <Skeleton className="h-3 w-24" />
              <Skeleton className="h-2 w-full" />
            </div>
            <Skeleton className="h-4 w-28" />
          </div>
        ) : !snapshot ? (
          <p className="text-sm text-muted-foreground">
            No data yet — connect a BMS on the Battery page.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div>
                <p className="text-xs text-muted-foreground">Voltage</p>
                <p className="text-lg font-semibold tabular-nums">{fmtNumber(snapshot.voltage, 2)} V</p>
              </div>
              <div>
                <p className="text-xs text-muted-foreground">Current</p>
                <p
                  className={`text-lg font-semibold tabular-nums ${
                    snapshot.current < 0 ? "text-sky-600 dark:text-sky-400" : "text-amber-600 dark:text-amber-400"
                  }`}
                >
                  {fmtSigned(snapshot.current, 2)} A
                </p>
              </div>
            </div>
            <div className="space-y-1">
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">SOC</span>
                <span className="font-semibold tabular-nums">{fmtNumber(snapshot.soc)}%</span>
              </div>
              <Progress value={snapshot.soc} />
            </div>
            <p className="text-sm text-muted-foreground">
              Time to 15%:{" "}
              <span className="font-medium text-foreground">{fmtHours(snapshot.time_to_15_hours)}</span>
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}
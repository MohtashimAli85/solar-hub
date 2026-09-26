import { Suspense, lazy } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState, Skeleton } from "@/components/ui/switch";
import type { BatteryHistoryPoint } from "@/hooks/useBatteryHistory";

const TrendChart = lazy(() =>
  import("@/components/battery/TrendChart").then((module) => ({ default: module.TrendChart })),
);

const COLORS = {
  soc: "oklch(0.81 0.117 11.638)", // --chart-1
  voltage: "oklch(0.586 0.253 17.585)", // --chart-3
  current: "oklch(0.455 0.188 13.697)", // --chart-5
};

interface BatteryTrendsProps {
  history: BatteryHistoryPoint[];
}

export function BatteryTrends({ history }: BatteryTrendsProps) {
  const collectable = history.length >= 2;
  return (
    <Card>
      <CardHeader>
        <CardTitle>Session trends</CardTitle>
        <CardDescription>Live telemetry since connection — resets on restart.</CardDescription>
      </CardHeader>
      <CardContent>
        {collectable ? (
          <Suspense fallback={<Skeleton className="h-28 w-full" />}>
            <div className="grid gap-4 lg:grid-cols-3">
            <TrendChart
              data={history}
              dataKey="soc"
              label="State of charge"
              unit="%"
              color={COLORS.soc}
              domain={[0, 100]}
            />
            <TrendChart
              data={history}
              dataKey="voltage"
              label="Voltage"
              unit="V"
              color={COLORS.voltage}
            />
            <TrendChart
              data={history}
              dataKey="current"
              label="Current"
              unit="A"
              color={COLORS.current}
            />
            </div>
          </Suspense>
        ) : (
          <EmptyState message="Trend data accumulates while connected — check back shortly." />
        )}
      </CardContent>
    </Card>
  );
}
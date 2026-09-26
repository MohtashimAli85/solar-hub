import { Suspense, lazy } from "react";
import { MetricCard } from "@/components/battery/MetricCard";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/switch";
import { fmtHours, fmtNumber, fmtSigned } from "@/lib/format";

const BatteryGauge = lazy(() =>
  import("@/components/battery/BatteryGauge").then((module) => ({ default: module.BatteryGauge })),
);

interface BatteryHeroProps {
  voltage: number;
  current: number;
  soc: number;
  timeTo15Hours: number | null;
}

export function BatteryHero({
  voltage,
  current,
  soc,
  timeTo15Hours,
}: BatteryHeroProps) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-6 sm:flex-row sm:items-centers p-5">
        <Suspense fallback={<Skeleton className="size-44 shrink-0 rounded-full" />}>
          <BatteryGauge soc={soc} current={current} />
        </Suspense>
        <div className="grid w-full grid-cols-2 gap-3 sm:grid-cols-3 sm:flex-1">
          <MetricCard label="Voltage" value={fmtNumber(voltage, 2)} unit="V" />
          <MetricCard
            label="Current"
            value={fmtSigned(current, 2)}
            unit="A"
            accent={
              current < -0.05
                ? "text-sky-600 dark:text-sky-400"
                : current > 0.05
                  ? "text-amber-600 dark:text-amber-400"
                  : undefined
            }
          />
          <MetricCard
            label="Time to 15%"
            value={fmtHours(timeTo15Hours)}
            sub={
              current > 0.05 ? "while charging or idle" : "at current demand"
            }
            className="col-span-2 sm:col-span-1"
          />
        </div>
      </CardContent>
    </Card>
  );
}

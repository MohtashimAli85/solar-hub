import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { CardRefreshHeader } from "@/components/inverter/CardRefreshHeader";
import { Skeleton } from "@/components/ui/switch";
import { fmtNumber } from "@/lib/format";
import type { DeviceDetails } from "@/lib/types";

interface DeviceSummaryCardProps {
  details: DeviceDetails | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  refreshing: boolean;
}

export function DeviceSummaryCard({
  details,
  loading,
  error,
  onRefresh,
  refreshing,
}: DeviceSummaryCardProps) {
  return (
    <Card>
      <CardRefreshHeader
        title="Device summary"
        description="Today, lifetime generation and environmental impact from the device record."
        onRefresh={onRefresh}
        refreshing={refreshing}
        action={
          details && !loading ? (
            <Badge variant={details.is_online ? "success" : "destructive"}>
              {details.is_online ? "Online" : "Offline"}
              {details.state_dict ? ` · ${details.state_dict}` : ""}
            </Badge>
          ) : null
        }
      />
      <CardContent>
        {loading ? (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {Array.from({ length: 6 }).map((_, index) => (
              <Skeleton key={index} className="h-16" />
            ))}
          </div>
        ) : error ? (
          <p className="text-sm text-destructive">{error}</p>
        ) : details ? (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <SummaryStat
              label="Today's generation"
              value={fmtNumber(details.daily_produced_quantity, 1)}
              unit="kWh"
            />
            <SummaryStat
              label="Lifetime generation"
              value={fmtNumber(details.total_produced_quantity, 1)}
              unit="kWh"
            />
            <SummaryStat
              label="Rated power"
              value={fmtNumber(details.rated_power, 2)}
              unit="kW"
            />
            <SummaryStat
              label="CO₂ offset"
              value={fmtNumber(details.co2_emission_reduction, 1)}
              unit="kg"
            />
            <SummaryStat
              label="SO₂ offset"
              value={fmtNumber(details.so2_emission_reduction, 1)}
              unit="kg"
            />
            <SummaryStat
              label="NOx offset"
              value={fmtNumber(details.nox_emission_reduction, 1)}
              unit="kg"
            />
            {details.station_name ? (
              <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-2">
                Station: {details.station_name}
              </p>
            ) : null}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">No device details available.</p>
        )}
      </CardContent>
    </Card>
  );
}

function SummaryStat({ label, value, unit }: { label: string; value: string; unit: string }) {
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-1 text-xl font-semibold tabular-nums">
        {value}
        <span className="ml-1 text-sm font-normal text-muted-foreground">{unit}</span>
      </p>
    </div>
  );
}
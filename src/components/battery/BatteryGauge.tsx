import { RadialBar, RadialBarChart, ResponsiveContainer } from "recharts";
import { Badge } from "@/components/ui/badge";
import { chargeState, socSeverity, type Severity } from "@/lib/batteryStatus";

const SEVERITY_FILL: Record<Severity, string> = {
  success: "#10b981",
  warning: "#f59e0b",
  destructive: "#ef4444",
};

interface BatteryGaugeProps {
  soc: number;
  current: number;
}

export function BatteryGauge({ soc, current }: BatteryGaugeProps) {
  const severity = socSeverity(soc);
  const state = chargeState(current);
  const stateBadge =
    state === "idle" ? (
      <Badge variant="secondary">Idle</Badge>
    ) : state === "charging" ? (
      <Badge variant="warning">Charging</Badge>
    ) : (
      <Badge variant="info">Discharging</Badge>
    );
  const data = [{ value: Math.round(soc) }];

  return (
    <div className="relative flex size-44 shrink-0 items-center justify-center">
      <ResponsiveContainer width="100%" height="100%">
        <RadialBarChart data={data} innerRadius="70%" outerRadius="100%" startAngle={210} endAngle={-30}>
          <RadialBar dataKey="value" fill={SEVERITY_FILL[severity]} background={{ fill: "var(--muted)" }} />
        </RadialBarChart>
      </ResponsiveContainer>
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-1">
        <p className="text-4xl font-bold tabular-nums">{Math.round(soc)}%</p>
        {stateBadge}
      </div>
    </div>
  );
}
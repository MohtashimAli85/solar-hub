import { Area, AreaChart, ResponsiveContainer, XAxis, YAxis } from "recharts";
import type { BatteryHistoryPoint } from "@/hooks/useBatteryHistory";

interface TrendChartProps {
  data: BatteryHistoryPoint[];
  dataKey: "soc" | "voltage" | "current";
  label: string;
  unit: string;
  color: string;
  domain?: [number | "auto", number | "auto"];
}

export function TrendChart({ data, dataKey, label, unit, color, domain }: TrendChartProps) {
  const gradientId = `trend-${dataKey}`;
  const formatted = data.map((point) => ({
    ...point,
    label: new Date(point.t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
  }));

  return (
    <div className="space-y-1">
      <div className="flex items-baseline justify-between text-sm">
        <span className="font-medium">{label}</span>
        {formatted.length > 0 ? (
          <span className="tabular-nums text-muted-foreground">
            {formatted[formatted.length - 1][dataKey].toFixed(1)} {unit}
          </span>
        ) : null}
      </div>
      <div className="h-28">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={formatted} margin={{ top: 4, right: 0, left: 0, bottom: 0 }}>
            <defs>
              <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={color} stopOpacity={0.35} />
                <stop offset="100%" stopColor={color} stopOpacity={0} />
              </linearGradient>
            </defs>
            <XAxis
              dataKey="label"
              tick={{ fontSize: 10 }}
              tickLine={false}
              axisLine={false}
              interval="preserveStartEnd"
              minTickGap={40}
            />
            <YAxis
              domain={domain ?? ["auto", "auto"]}
              tick={{ fontSize: 10 }}
              tickLine={false}
              axisLine={false}
              width={36}
            />
            <Area
              type="monotone"
              dataKey={dataKey}
              stroke={color}
              strokeWidth={2}
              fill={`url(#${gradientId})`}
              isAnimationActive={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
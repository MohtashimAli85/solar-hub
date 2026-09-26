import { AlertTriangle } from "lucide-react";

interface AlarmBannerProps {
  alarms: unknown[];
}

export function AlarmBanner({ alarms }: AlarmBannerProps) {
  if (!alarms || alarms.length === 0) return null;

  return (
    <div className="rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm">
      <div className="flex items-center gap-2 font-medium text-destructive">
        <AlertTriangle className="size-4" />
        {alarms.length === 1 ? "1 active alarm" : `${alarms.length} active alarms`}
      </div>
      <ul className="mt-2 space-y-1">
        {alarms.map((alarm, index) => (
          <li key={index} className="text-destructive">
            {describeAlarm(alarm)}
          </li>
        ))}
      </ul>
    </div>
  );
}

function describeAlarm(alarm: unknown): string {
  if (alarm && typeof alarm === "object") {
    const entry = alarm as Record<string, unknown>;
    const text =
      entry["name"] ?? entry["message"] ?? entry["nameDisplay"] ?? entry["valueDisplay"];
    if (typeof text === "string" && text.trim() !== "") return text;
  }
  return JSON.stringify(alarm);
}
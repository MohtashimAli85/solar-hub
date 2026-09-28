import { Switch } from "@/components/ui/switch";
import { OUTPUT_MODES } from "@/lib/types";
import type { InverterSettings } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";

const MODE_HINTS: Record<string, string> = {
  "0": "Solar: panels first, then grid. The battery is only used when both are gone.",
  "1": "SBG: panels first, then battery, then grid.",
  "2": "Utility: grid first, then battery. The panels don't power the house.",
};

interface OutputSourcePanelProps {
  settings: InverterSettings | null;
  pending: boolean;
  smartLoadPending: boolean;
  onSet: (mode: string) => void;
  onSetSmartLoad: (enabled: boolean) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  disabled?: boolean;
  title?: string;
  compact?: boolean;
}

export function OutputSourcePanel({
  settings,
  pending,
  smartLoadPending,
  onSet,
  onSetSmartLoad,
  disabled,
  title = "Output source",
  compact = false,
}: OutputSourcePanelProps) {
  const currentValue = settings?.output_source_priority_value;
  const smartLoadUnsupported = settings?.smart_load == null;
  const smartLoadOn = settings?.smart_load === 1;

  return (
    <section
      className={cn(
        "rounded-xl border border-border bg-card",
        compact ? "p-4" : "p-4 sm:p-5",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-label font-semibold uppercase tracking-wide text-muted-foreground">
          {title}
        </h3>
        {pending || smartLoadPending ? (
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" /> Saving…
          </span>
        ) : (
          <span className="text-xs tabular-nums text-muted-foreground">
            charger <span className="text-foreground">{settings?.charger_source_priority ?? "–"}</span>
          </span>
        )}
      </div>

      <div
        role="tablist"
        aria-label="Output source priority"
        className="mt-3 grid grid-cols-3 gap-1 rounded-lg border border-border bg-muted p-1"
      >
        {OUTPUT_MODES.map((mode) => {
          const active = currentValue != null && String(currentValue) === mode.value;
          return (
            <button
              key={mode.value}
              role="tab"
              aria-selected={active}
              disabled={disabled || pending}
              onClick={() => onSet(mode.value)}
              className={cn(
                "rounded-md px-1.5 py-2 text-xs font-semibold tracking-wide transition-colors sm:px-2 sm:text-sm pointer-coarse:py-3 disabled:cursor-not-allowed disabled:opacity-50",
                active
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
            >
              {mode.label}
            </button>
          );
        })}
      </div>
      <p className="mt-2 text-micro leading-snug text-muted-foreground">
        {MODE_HINTS[String(currentValue ?? "")] ?? "Pick where the house draws power from."} Changing it by hand pauses automation until the next window.
      </p>

      <div
        className={cn(
          "mt-3 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5",
          (disabled || smartLoadPending) && "opacity-60",
        )}
      >
        <div>
          <p className="text-sm font-medium">
            Smart load{" "}
            <span className={cn("text-xs", smartLoadOn ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground")}>
              {smartLoadOn ? "on — heavy loads cut" : "off — everything runs"}
            </span>
          </p>
          <p className="text-micro text-muted-foreground">
            {smartLoadUnsupported
              ? "Not supported by this device"
              : "On cuts heavy and non-UPS loads to protect the battery. Off lets everything run."}
          </p>
        </div>
        <Switch
          checked={smartLoadOn}
          disabled={disabled || smartLoadPending || smartLoadUnsupported}
          onCheckedChange={(next) => {
            if (!(disabled || smartLoadPending)) onSetSmartLoad(next);
          }}
        />
      </div>
    </section>
  );
}

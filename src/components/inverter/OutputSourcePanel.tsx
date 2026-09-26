import { Switch } from "@/components/ui/switch";
import { OUTPUT_MODES } from "@/lib/types";
import type { InverterSettings } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";

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
        compact ? "p-4" : "p-5",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
          {title}
        </h3>
        {pending || smartLoadPending ? (
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" /> Saving…
          </span>
        ) : (
          <span className="text-xs tabular-nums text-muted-foreground">
            {settings ? settings.output_source_priority : "–"}
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
                "rounded-md px-2 py-2 text-sm font-semibold tracking-wide transition-colors disabled:cursor-not-allowed disabled:opacity-50",
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
      <p className="mt-2 text-[11px] leading-snug text-muted-foreground">
        Manual changes pause automation until the next window.
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
            <span className={cn("text-xs", smartLoadOn ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground")}>
              {smartLoadOn ? "on" : "off"}
            </span>
          </p>
          {smartLoadUnsupported ? (
            <p className="text-[11px] text-muted-foreground">Not supported by this device</p>
          ) : null}
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

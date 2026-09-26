import { Card, CardContent } from "@/components/ui/card";
import { CardRefreshHeader } from "@/components/inverter/CardRefreshHeader";
import { Switch } from "@/components/ui/switch";
import { fmtNumber } from "@/lib/format";
import type { InverterSettings } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

type SettingCommit<T> = { pending: boolean; onCommit: (value: T) => void };

interface SettingsControlsProps {
  settings: InverterSettings | null;
  onRefresh: () => void;
  refreshing: boolean;
  onSetAcInput: SettingCommit<boolean>;
  onSetFeedIn: SettingCommit<boolean>;
  onSetLowCutoff: SettingCommit<number>;
  onSetHighCutoff: SettingCommit<number>;
  onSetLowSoc: SettingCommit<number>;
  onSetMaxCharge: SettingCommit<number>;
  onSetMaxUtilityCharge: SettingCommit<number>;
}

export function SettingsControls({
  settings,
  onRefresh,
  refreshing,
  onSetAcInput,
  onSetFeedIn,
  onSetLowCutoff,
  onSetHighCutoff,
  onSetLowSoc,
  onSetMaxCharge,
  onSetMaxUtilityCharge,
}: SettingsControlsProps) {
  return (
    <Card>
      <CardRefreshHeader
        title="Device settings"
        description="Battery protection and grid behaviour — unavailable controls are disabled for this firmware."
        onRefresh={onRefresh}
        refreshing={refreshing}
      />
      <CardContent className="space-y-5">
        <div className="grid grid-cols-1 gap-5 sm:grid-cols-3">
          <SettingSlider
            label="Low battery cutoff voltage"
            value={settings?.low_battery_cutoff_voltage ?? null}
            min={20}
            max={30}
            step={0.1}
            digits={1}
            unit="V"
            commit={onSetLowCutoff}
          />
          <SettingSlider
            label="High cutoff voltage"
            value={settings?.high_cutoff_voltage ?? null}
            min={20}
            max={40}
            step={0.1}
            digits={1}
            unit="V"
            commit={onSetHighCutoff}
          />
          <SettingSlider
            label="Low DC cutoff SOC"
            value={settings?.low_dc_cutoff_soc ?? null}
            min={0}
            max={100}
            step={1}
            unit="%"
            commit={onSetLowSoc}
          />
          <SettingSlider
            label="Max total charge current"
            value={settings?.max_total_charge_current ?? null}
            min={0}
            max={100}
            step={1}
            unit="A"
            commit={onSetMaxCharge}
          />
          <SettingSlider
            label="Max utility charge current"
            value={settings?.max_utility_charge_current ?? null}
            min={0}
            max={30}
            step={1}
            unit="A"
            commit={onSetMaxUtilityCharge}
          />
        </div>
        <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
          <SettingToggle
            label="Grid charging"
            description="Appliance mode allows charging from the utility; UPS mode bypasses."
            value={settings?.ac_input_range ?? null}
            onValue={0}
            commit={onSetAcInput}
          />
          <SettingToggle
            label="Grid feed-in"
            description="Export surplus solar to the grid."
            value={settings?.battery_power_limiting ?? null}
            onValue={1}
            commit={onSetFeedIn}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function ControlCaption({ pending, unsupported }: { pending: boolean; unsupported: boolean }) {
  if (pending) {
    return (
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Loader2 className="size-3 animate-spin" /> Saving…
      </p>
    );
  }
  if (unsupported) {
    return <p className="text-xs text-muted-foreground">Not supported by this device</p>;
  }
  return null;
}

function SettingSlider({
  label,
  value,
  min,
  max,
  step,
  digits = 0,
  unit,
  commit,
}: {
  label: string;
  value: number | null;
  min: number;
  max: number;
  step: number;
  digits?: number;
  unit: string;
  commit: SettingCommit<number>;
}) {
  const { pending, onCommit } = commit;
  const unsupported = value === null;
  const disabled = unsupported || pending;
  const [draft, setDraft] = useState(value ?? min);
  useEffect(() => {
    if (value != null) setDraft(value);
  }, [value]);

  const commitDraft = () => {
    if (disabled || draft === value) return;
    onCommit(draft);
  };

  return (
    <div className={cn("space-y-2", disabled && "opacity-60")}>
      <div className="flex items-center justify-between gap-2 text-sm">
        <span className="font-medium">{label}</span>
        <span className="tabular-nums text-muted-foreground">
          {fmtNumber(draft, digits)} {unit}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={draft}
        disabled={disabled}
        onChange={(event) => setDraft(Number(event.target.value))}
        onPointerUp={commitDraft}
        onKeyUp={commitDraft}
        className="w-full accent-primary"
      />
      <ControlCaption pending={pending} unsupported={unsupported} />
    </div>
  );
}

function SettingToggle({
  label,
  description,
  value,
  onValue,
  commit,
}: {
  label: string;
  description: string;
  value: number | null;
  onValue: number;
  commit: SettingCommit<boolean>;
}) {
  const { pending, onCommit } = commit;
  const unsupported = value === null;
  const disabled = unsupported || pending;
  const checked = value === onValue;

  return (
    <div className={cn("flex items-start justify-between gap-3 rounded-lg border border-border p-4", disabled && "opacity-60")}>
      <div className="space-y-1">
        <p className="text-sm font-medium">{label}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
        <ControlCaption pending={pending} unsupported={unsupported} />
      </div>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={(next) => {
          if (!disabled) onCommit(next);
        }}
      />
    </div>
  );
}
import { OutputSourcePanel } from "@/components/inverter/OutputSourcePanel";
import type { InverterSettings } from "@/lib/types";

interface PrioritySelectorProps {
  settings: InverterSettings | null;
  pending: boolean;
  smartLoadPending: boolean;
  onSet: (mode: string) => void;
  onSetSmartLoad: (enabled: boolean) => void;
  onRefresh: () => void;
  refreshing: boolean;
  disabled?: boolean;
}

export function PrioritySelector(props: PrioritySelectorProps) {
  return <OutputSourcePanel {...props} title="Output priority override" />;
}

import { Loader2, MonitorOff, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useLink } from "@/hooks/useLink";
import { getAutomationDecisions } from "@/lib/tauri";
import { listen } from "@/lib/transport";
import type { AutomationDecision } from "@/lib/types";

function sinceLabel(iso: string | null) {
  if (!iso) return "";
  const at = new Date(iso);
  const sameDay = at.toDateString() === new Date().toDateString();
  const time = at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return sameDay ? ` since ${time}` : ` since ${at.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

export function LinkBanner() {
  const link = useLink();

  if (link.mode === "starting") {
    return (
      <div className="mb-4">
        <Badge variant="secondary" role="status">
          <Loader2 className="size-3 animate-spin" /> Looking for the Mac…
        </Badge>
      </div>
    );
  }
  if (link.mode === "mac") return null;
  return (
    <div role="status" className="mb-4 flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted px-3 py-2 text-sm">
      <Badge variant="warning">
        <MonitorOff className="size-3" /> Mac offline{sinceLabel(link.offlineSince)}
      </Badge>
      <span className="text-muted-foreground">
        Battery and inverter are read by this phone. Plans and decisions are the Mac's last ones.
      </span>
    </div>
  );
}

export function DecisionBanner() {
  const link = useLink();
  const [decision, setDecision] = useState<AutomationDecision | null>(null);
  const lastSeen = useRef<string | null>(null);

  useEffect(() => {
    if (link.mode !== "mac") return;
    let cancelled = false;
    const check = async () => {
      const [latest] = await getAutomationDecisions(1).catch(() => [] as AutomationDecision[]);
      if (cancelled || !latest) return;
      if (lastSeen.current != null && latest.at !== lastSeen.current) setDecision(latest);
      lastSeen.current = latest.at;
    };
    void check();
    const unlisten = listen("automation://status", () => void check());
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, [link.mode]);

  useEffect(() => {
    if (!decision) return;
    const timer = setTimeout(() => setDecision(null), 15_000);
    return () => clearTimeout(timer);
  }, [decision]);

  if (!decision) return null;
  return (
    <div
      role="status"
      className="fixed inset-x-4 top-[max(0.75rem,env(safe-area-inset-top))] z-50 mx-auto flex max-w-md items-start gap-3 rounded-xl border border-border bg-card p-3 shadow-lg"
    >
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={decision.applied ? "default" : "secondary"}>{decision.mode}</Badge>
          <span className="text-xs text-muted-foreground">Mac decided</span>
        </div>
        <p className="line-clamp-3 text-sm">{decision.reason}</p>
      </div>
      <Button variant="ghost" size="icon" aria-label="Dismiss" onClick={() => setDecision(null)}>
        <X />
      </Button>
    </div>
  );
}

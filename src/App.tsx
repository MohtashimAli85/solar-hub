import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Battery, BrainCircuit, Gauge, Moon, Settings, Sun } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { cn } from "@/lib/utils";
import { useTheme } from "@/hooks/useTheme";
import { Button } from "@/components/ui/button";
import { DashboardPage } from "@/pages/DashboardPage";

const AutomationPage = lazy(() => import("@/pages/AutomationPage").then((m) => ({ default: m.AutomationPage })));
const BatteryPage = lazy(() => import("@/pages/BatteryPage").then((m) => ({ default: m.BatteryPage })));
const InverterPage = lazy(() => import("@/pages/InverterPage").then((m) => ({ default: m.InverterPage })));
const SettingsPage = lazy(() => import("@/pages/SettingsPage").then((m) => ({ default: m.SettingsPage })));

function PageFallback() {
  return <div className="h-40 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
}

type Tab = "dashboard" | "battery" | "inverter" | "automation" | "settings";

const TABS: { id: Tab; label: string; icon: typeof Gauge }[] = [
  { id: "dashboard", label: "Dashboard", icon: Gauge },
  { id: "battery", label: "Battery", icon: Battery },
  { id: "inverter", label: "Inverter", icon: Sun },
  { id: "automation", label: "Automation", icon: BrainCircuit },
  { id: "settings", label: "Settings", icon: Settings },
];

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5_000 },
  },
});

function Shell() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const { theme, toggleTheme } = useTheme();

  return (
    <div className="flex h-dvh">
      <aside className="flex w-52 shrink-0 flex-col border-r border-border bg-card">
        <div className="flex items-center gap-2 px-4 py-5">
          <span className="flex size-8 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <Sun className="size-4" />
          </span>
          <div>
            <p className="text-sm font-semibold leading-tight">Solar Hub</p>
            <p className="text-xs text-muted-foreground">battery + inverter</p>
          </div>
        </div>
        <nav className="flex flex-col gap-1 px-2">
          {TABS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              aria-current={tab === id ? "page" : undefined}
              className={cn(
                "flex items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                tab === id
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
              )}
            >
              <Icon className="size-4" />
              {label}
            </button>
          ))}
        </nav>
        <div className="mt-auto flex items-center justify-between px-4 py-4">
          <p className="text-xs text-muted-foreground">Theme</p>
          <Button
            variant="ghost"
            size="icon"
            onClick={toggleTheme}
            aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          >
            {theme === "dark" ? <Sun className="size-4" /> : <Moon className="size-4" />}
          </Button>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col overflow-y-auto p-6">
        <Suspense fallback={<PageFallback />}>
          {tab === "dashboard" ? <DashboardPage onOpenAutomation={() => setTab("automation")} /> : null}
          {tab === "battery" ? <BatteryPage /> : null}
          {tab === "inverter" ? <InverterPage /> : null}
          {tab === "automation" ? <AutomationPage onOpenSettings={() => setTab("settings")} /> : null}
          {tab === "settings" ? <SettingsPage /> : null}
        </Suspense>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}

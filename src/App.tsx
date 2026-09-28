import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { lazy, Suspense, useEffect, useState } from "react";
import { useTheme } from "@/hooks/useTheme";
import { BottomNav, MobileHeader, NavRail, Sidebar, type Tab } from "@/components/layout/AppNav";
import { PairScreen } from "@/components/remote/PairScreen";
import { hasPhoneToken, isDesktop, onPairingNeeded } from "@/lib/transport";
import { DashboardPage } from "@/pages/DashboardPage";

const AutomationPage = lazy(() => import("@/pages/AutomationPage").then((m) => ({ default: m.AutomationPage })));
const BatteryPage = lazy(() => import("@/pages/BatteryPage").then((m) => ({ default: m.BatteryPage })));
const InverterPage = lazy(() => import("@/pages/InverterPage").then((m) => ({ default: m.InverterPage })));
const SettingsPage = lazy(() => import("@/pages/SettingsPage").then((m) => ({ default: m.SettingsPage })));

function PageFallback() {
  return <div className="h-40 animate-pulse rounded-xl border border-border bg-card motion-reduce:animate-none" />;
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5_000 },
  },
});

function Shell() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [paired, setPaired] = useState(() => isDesktop || hasPhoneToken());
  const { theme, toggleTheme } = useTheme();

  useEffect(() => {
    if (isDesktop) return;
    return onPairingNeeded(() => setPaired(false));
  }, []);

  if (!paired) {
    return (
      <PairScreen
        onPaired={() => {
          queryClient.clear();
          setPaired(true);
        }}
      />
    );
  }

  return (
    <div className="flex h-dvh">
      <Sidebar tab={tab} onSelect={setTab} theme={theme} onToggleTheme={toggleTheme} />
      <NavRail tab={tab} onSelect={setTab} theme={theme} onToggleTheme={toggleTheme} />

      <div className="flex min-w-0 flex-1 flex-col">
        <MobileHeader theme={theme} onToggleTheme={toggleTheme} />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto p-4 md:p-5 lg:p-6">
          <div className="mx-auto w-full max-w-[1600px]">
            <Suspense fallback={<PageFallback />}>
              {tab === "dashboard" ? <DashboardPage onOpenAutomation={() => setTab("automation")} /> : null}
              {tab === "battery" ? <BatteryPage /> : null}
              {tab === "inverter" ? <InverterPage /> : null}
              {tab === "automation" ? <AutomationPage onOpenSettings={() => setTab("settings")} /> : null}
              {tab === "settings" ? <SettingsPage /> : null}
            </Suspense>
          </div>
        </main>
        <BottomNav tab={tab} onSelect={setTab} />
      </div>
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

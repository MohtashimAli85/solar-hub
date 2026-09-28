import { Battery, BrainCircuit, Gauge, Moon, Settings, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Theme } from "@/hooks/useTheme";
import { cn } from "@/lib/utils";

export type Tab = "dashboard" | "battery" | "inverter" | "automation" | "settings";

export const TABS: { id: Tab; label: string; icon: typeof Gauge }[] = [
  { id: "dashboard", label: "Dashboard", icon: Gauge },
  { id: "battery", label: "Battery", icon: Battery },
  { id: "inverter", label: "Inverter", icon: Sun },
  { id: "automation", label: "Automation", icon: BrainCircuit },
  { id: "settings", label: "Settings", icon: Settings },
];

interface NavProps {
  tab: Tab;
  onSelect: (tab: Tab) => void;
}

interface ThemeProps {
  theme: Theme;
  onToggleTheme: () => void;
}

function Logo() {
  return (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground">
      <Sun className="size-4" />
    </span>
  );
}

function ThemeButton({ theme, onToggleTheme }: ThemeProps) {
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={onToggleTheme}
      aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
    >
      {theme === "dark" ? <Sun className="size-4" /> : <Moon className="size-4" />}
    </Button>
  );
}

export function Sidebar({ tab, onSelect, theme, onToggleTheme }: NavProps & ThemeProps) {
  return (
    <aside className="hidden w-52 shrink-0 flex-col border-r border-border bg-card lg:flex">
      <div className="flex items-center gap-2 px-4 py-5">
        <Logo />
        <div>
          <p className="text-sm font-semibold leading-tight">Solar Hub</p>
          <p className="text-xs text-muted-foreground">battery + inverter</p>
        </div>
      </div>
      <nav className="flex flex-col gap-1 px-2">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => onSelect(id)}
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
        <ThemeButton theme={theme} onToggleTheme={onToggleTheme} />
      </div>
    </aside>
  );
}

export function NavRail({ tab, onSelect, theme, onToggleTheme }: NavProps & ThemeProps) {
  return (
    <aside className="hidden w-20 shrink-0 flex-col items-center border-r border-border bg-card py-4 md:flex lg:hidden">
      <Logo />
      <nav className="mt-6 flex flex-col gap-1">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => onSelect(id)}
            aria-current={tab === id ? "page" : undefined}
            className={cn(
              "flex w-16 flex-col items-center gap-1 rounded-md py-2 text-micro font-medium transition-colors",
              tab === id
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
            )}
          >
            <Icon className="size-5" />
            {label}
          </button>
        ))}
      </nav>
      <div className="mt-auto">
        <ThemeButton theme={theme} onToggleTheme={onToggleTheme} />
      </div>
    </aside>
  );
}

export function MobileHeader({ theme, onToggleTheme }: ThemeProps) {
  return (
    <header className="flex items-center justify-between border-b border-border bg-card px-4 pb-2 pt-[max(0.5rem,env(safe-area-inset-top))] md:hidden">
      <div className="flex items-center gap-2">
        <Logo />
        <p className="text-sm font-semibold">Solar Hub</p>
      </div>
      <ThemeButton theme={theme} onToggleTheme={onToggleTheme} />
    </header>
  );
}

export function BottomNav({ tab, onSelect }: NavProps) {
  return (
    <nav className="shrink-0 border-t border-border bg-card pb-[env(safe-area-inset-bottom)] md:hidden">
      <div className="grid grid-cols-5">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => onSelect(id)}
            aria-current={tab === id ? "page" : undefined}
            className={cn(
              "flex h-16 flex-col items-center justify-center gap-1 text-micro font-medium transition-colors",
              tab === id ? "text-primary" : "text-muted-foreground",
            )}
          >
            <Icon className="size-5" />
            <span className="max-w-full truncate px-1">{label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { isDesktop } from "@/lib/transport";
import type { AutomationInsights } from "@/lib/types";
import { FolderOpen } from "lucide-react";
import { SectionTitle } from "./shared";

interface RecordsCardProps {
  insights: AutomationInsights | undefined;
  fallbackDir: string;
  opening: boolean;
  error: string | null;
  onOpen: () => void;
}

export function RecordsCard({ insights, fallbackDir, opening, error, onOpen }: RecordsCardProps) {
  const dir = insights?.records.dir || fallbackDir;
  return (
    <Card>
      <CardHeader className="gap-1">
        <SectionTitle eyebrow="Records" title="Every reading and decision, saved as CSV" />
        <p className="text-xs text-muted-foreground">One file per month, only ever added to — nothing is deleted.</p>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="rounded-md border border-border bg-muted/40 px-3 py-2 font-mono text-xs break-all">{dir}</div>
        {insights ? (
          <p className="text-sm text-muted-foreground tabular-nums">
            This month: <span className="text-foreground">{insights.records.samples_this_month.toLocaleString()}</span> readings ·{" "}
            <span className="text-foreground">{insights.records.decisions_this_month.toLocaleString()}</span> decisions
          </p>
        ) : null}
        {isDesktop ? (
          <Button variant="outline" size="sm" onClick={onOpen} disabled={opening}>
            <FolderOpen /> Open folder
          </Button>
        ) : null}
        {error ? <p className="text-xs text-destructive">{error}</p> : null}
        <details className="group rounded-md border border-border px-3 py-2 text-sm">
          <summary className="cursor-pointer select-none font-medium">Connect Excel</summary>
          <ol className="mt-2 list-decimal space-y-1 pl-5 text-muted-foreground">
            <li>
              In Excel: <span className="text-foreground">Data → Get Data → From File → From Folder</span>, and pick the{" "}
              <span className="font-mono text-foreground">samples</span> folder (or <span className="font-mono text-foreground">decisions</span>).
            </li>
            <li>
              Choose <span className="text-foreground">Combine &amp; Load</span>. Every month's file becomes one table.
            </li>
            <li>
              Press <span className="text-foreground">Refresh</span> whenever you want the newest rows.
            </li>
            <li>Don't open a CSV directly and save over it — Excel can rewrite the format while the app is still adding rows.</li>
          </ol>
        </details>
      </CardContent>
    </Card>
  );
}

import { Button } from "@/components/ui/button";
import { CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { RefreshCw } from "lucide-react";
import type { ReactNode } from "react";

interface CardRefreshHeaderProps {
  title: string;
  description?: string;
  onRefresh: () => void;
  refreshing: boolean;
  action?: ReactNode;
}

export function CardRefreshHeader({
  title,
  description,
  onRefresh,
  refreshing,
  action,
}: CardRefreshHeaderProps) {
  return (
    <CardHeader className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
      <div className="space-y-1">
        <CardTitle>{title}</CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {action}
        <Button
          variant="ghost"
          size="icon"
          onClick={onRefresh}
          disabled={refreshing}
          aria-label={`Refresh ${title}`}
        >
          <RefreshCw className={refreshing ? "animate-spin" : ""} />
        </Button>
      </div>
    </CardHeader>
  );
}
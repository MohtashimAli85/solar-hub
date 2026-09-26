import { Badge } from "@/components/ui/badge";
import {
  parseBalanceStatus,
  parseFetStatus,
  parseProtectionFlags,
} from "@/lib/batteryStatus";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { CheckCircle2, HeartPulse, ShieldAlert, SlidersHorizontal } from "lucide-react";

interface StatusBadgesProps {
  fetStatus: number | null | undefined;
  balanceStatus: number;
  protectionStatus: number;
}

export function StatusBadges({ fetStatus, balanceStatus, protectionStatus }: StatusBadgesProps) {
  const fet = parseFetStatus(fetStatus);
  const balance = parseBalanceStatus(balanceStatus);
  const protection = parseProtectionFlags(protectionStatus);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Status</CardTitle>
        <CardDescription>Protection, FET and balancing state from the BMS.</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap gap-2">
          <Badge variant={fet.charge ? "success" : "secondary"}>
            <CheckCircle2 className="size-3" />
            {fet.charge ? "Charge FET on" : "Charge FET off"}
          </Badge>
          <Badge variant={fet.discharge ? "success" : "secondary"}>
            <CheckCircle2 className="size-3" />
            {fet.discharge ? "Discharge FET on" : "Discharge FET off"}
          </Badge>
          <Badge variant={balance.count === 0 ? "secondary" : "info"}>
            <SlidersHorizontal className="size-3" />
            {balance.count === 0
              ? "Balanced"
              : `Balancing ${balance.count} cell${balance.count === 1 ? "" : "s"}`}
          </Badge>
          {protection.length === 0 ? (
            <Badge variant="success">
              <HeartPulse className="size-3" />
              Protection normal
            </Badge>
          ) : (
            protection.map((flag) => (
              <Badge key={flag.bit} variant="destructive">
                <ShieldAlert className="size-3" />
                {flag.label}
              </Badge>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  );
}
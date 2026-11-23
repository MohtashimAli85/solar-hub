import { RefreshCw, Smartphone, Unlink } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useLink } from "@/hooks/useLink";
import { findMac, getMacAddress, startAndroidLink, unpairPhone } from "@/lib/transport";

export function PhoneLinkCard() {
  const link = useLink();
  const [searching, setSearching] = useState(false);
  const [notFound, setNotFound] = useState(false);
  const address = getMacAddress()?.replace(/^https?:\/\//, "") ?? "—";

  const search = async () => {
    setSearching(true);
    setNotFound(false);
    const found = await findMac().catch(() => null);
    if (found) await startAndroidLink();
    setNotFound(!found);
    setSearching(false);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Smartphone className="size-4" /> This phone
        </CardTitle>
        <CardDescription>
          Shows what the Mac is doing. When the Mac is off, it reads the battery over Bluetooth and the inverter from the
          cloud by itself. It never switches anything on its own.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="text-muted-foreground">Mac</span>
          <span className="font-mono">{address}</span>
          <Badge variant={link.mode === "mac" ? "success" : "warning"}>
            {link.mode === "mac" ? "Connected" : link.mode === "starting" ? "Checking…" : "Offline"}
          </Badge>
        </div>
        {notFound ? <p className="text-sm text-muted-foreground">Couldn't find the Mac on this Wi-Fi.</p> : null}
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => void search()} disabled={searching}>
            <RefreshCw className={searching ? "animate-spin" : undefined} /> {searching ? "Searching…" : "Find Mac again"}
          </Button>
          <Button variant="outline" onClick={unpairPhone}>
            <Unlink /> Unpair
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

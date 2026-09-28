import { Check, Copy, RefreshCw, Smartphone } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { useRemoteAccess } from "@/hooks/useRemoteAccess";

export function PhoneAccessCard() {
  const { status, error, toggle, regeneratePin } = useRemoteAccess();
  const [copied, setCopied] = useState(false);
  const url = status?.urls[0] ?? null;
  const pairUrl = url && status ? `${url}/#pin=${status.pin}` : null;
  const actionError = toggle.error ?? regeneratePin.error;

  const copy = async () => {
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="space-y-1">
            <CardTitle className="flex items-center gap-2">
              <Smartphone className="size-4" /> Phone access
            </CardTitle>
            <CardDescription>
              Open Solar Hub on a phone or tablet on the same Wi-Fi. This Mac must stay on with the app open.
            </CardDescription>
          </div>
          <Switch
            aria-label="Phone access"
            checked={status?.enabled ?? false}
            disabled={!status || toggle.isPending}
            onCheckedChange={(enabled) => toggle.mutate(enabled)}
          />
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        {actionError ? <p className="text-sm text-destructive">{String(actionError)}</p> : null}
        {status?.error ? <p className="text-sm text-destructive">{status.error}</p> : null}

        {status?.enabled && status.running ? (
          <div className="flex flex-col gap-5 sm:flex-row sm:items-start">
            {pairUrl ? (
              <div className="self-center rounded-lg bg-white p-3 sm:self-start">
                <QRCodeSVG value={pairUrl} size={148} marginSize={0} aria-label="QR code that opens and pairs Solar Hub on a phone" />
              </div>
            ) : null}
            <div className="min-w-0 flex-1 space-y-4">
              <div className="space-y-1">
                <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">Address</p>
                {url ? (
                  <div className="flex items-center gap-2">
                    <code className="truncate rounded-md border border-border bg-muted/40 px-2 py-1 text-sm">{url}</code>
                    <Button variant="ghost" size="icon" onClick={copy} aria-label="Copy address">
                      {copied ? <Check /> : <Copy />}
                    </Button>
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">Could not find this Mac's Wi-Fi address. Check that Wi-Fi is on.</p>
                )}
              </div>
              <div className="space-y-1">
                <p className="text-micro font-medium uppercase tracking-wide text-muted-foreground">PIN</p>
                <div className="flex items-center gap-3">
                  <p className="font-mono text-2xl font-semibold tracking-[0.3em] tabular-nums">{status.pin}</p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => regeneratePin.mutate()}
                    disabled={regeneratePin.isPending}
                  >
                    <RefreshCw /> New PIN
                  </Button>
                </div>
                <p className="text-xs text-muted-foreground">
                  {status.paired_devices === 0
                    ? "No phones paired yet."
                    : `${status.paired_devices} ${status.paired_devices === 1 ? "phone" : "phones"} paired.`}{" "}
                  A new PIN unpairs every phone.
                </p>
              </div>
              <p className="text-xs text-muted-foreground">
                Scan the code with the phone camera, or open the address and type the PIN. Passwords, API keys and
                Bluetooth pairing can only be changed here on the Mac.
              </p>
            </div>
          </div>
        ) : status?.enabled ? (
          status.error ? null : <p className="text-sm text-muted-foreground">Starting…</p>
        ) : (
          <p className="text-sm text-muted-foreground">Off. Turn it on to get an address and PIN for your phone.</p>
        )}
      </CardContent>
    </Card>
  );
}

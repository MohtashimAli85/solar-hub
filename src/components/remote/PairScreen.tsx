import { useEffect, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input, Label } from "@/components/ui/input";
import { SolarHubMark } from "@/components/layout/SolarHubMark";
import { findMac, getMacAddress, isAndroidApp, pairPhone, setMacAddress } from "@/lib/transport";

function takePinFromUrl(): string | null {
  const match = window.location.hash.match(/pin=(\d{6})/);
  if (!match) return null;
  window.history.replaceState(null, "", window.location.pathname + window.location.search);
  return match[1];
}

export function PairScreen({ onPaired }: { onPaired: () => void }) {
  const [pin, setPin] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const autoTried = useRef(false);
  const [mac, setMac] = useState<string | null>(() => (isAndroidApp ? getMacAddress() : null));
  const [searching, setSearching] = useState(false);
  const [address, setAddress] = useState("");

  const search = async () => {
    setSearching(true);
    setError(null);
    const found = await findMac().catch(() => null);
    setSearching(false);
    setMac(found);
    if (!found) setError("Couldn't find Solar Hub on this Wi-Fi. Check the Mac is on and Phone access is turned on, or type its address.");
  };

  const submitAddress = (event: FormEvent) => {
    event.preventDefault();
    try {
      setMacAddress(address);
      setMac(getMacAddress());
      setError(null);
    } catch {
      setError("That address doesn't look right. Example: 192.168.1.20");
    }
  };

  const submit = async (value: string) => {
    setPending(true);
    setError(null);
    try {
      await pairPhone(value);
      onPaired();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  useEffect(() => {
    if (autoTried.current) return;
    autoTried.current = true;
    if (isAndroidApp) {
      void search();
      return;
    }
    const fromUrl = takePinFromUrl();
    if (fromUrl) {
      setPin(fromUrl);
      void submit(fromUrl);
    }
  }, []);

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (pin.length === 6) void submit(pin);
  };

  if (isAndroidApp && !mac) {
    return (
      <div className="flex h-dvh justify-center overflow-y-auto bg-background p-4">
        <form onSubmit={submitAddress} className="my-auto w-full max-w-sm space-y-5 rounded-xl border border-border bg-card p-6">
          <div className="flex items-center gap-3">
            <span className="flex size-10 items-center justify-center rounded-md bg-primary text-primary-foreground">
              <SolarHubMark className="size-6" />
            </span>
            <div>
              <h1 className="text-lg font-semibold leading-tight">Solar Hub</h1>
              <p className="text-sm text-muted-foreground">
                {searching ? "Looking for your Mac on this Wi-Fi…" : "Connect to your Mac"}
              </p>
            </div>
          </div>
          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
          <Button type="button" className="w-full" size="lg" disabled={searching} onClick={() => void search()}>
            {searching ? "Searching…" : "Search again"}
          </Button>
          <div className="space-y-1.5">
            <Label htmlFor="mac-address">Mac address</Label>
            <Input
              id="mac-address"
              inputMode="url"
              placeholder="192.168.1.20"
              value={address}
              onChange={(event) => setAddress(event.target.value)}
            />
            <p className="text-xs text-muted-foreground">Shown on the Mac in Solar Hub → Settings → Phone access.</p>
          </div>
          <Button type="submit" variant="outline" className="w-full" disabled={searching || !address.trim()}>
            Use this address
          </Button>
        </form>
      </div>
    );
  }

  return (
    <div className="flex h-dvh justify-center overflow-y-auto bg-background p-4">
      <form onSubmit={onSubmit} className="my-auto w-full max-w-sm space-y-5 rounded-xl border border-border bg-card p-6">
        <div className="flex items-center gap-3">
          <span className="flex size-10 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <SolarHubMark className="size-6" />
          </span>
          <div>
            <h1 className="text-lg font-semibold leading-tight">Solar Hub</h1>
            <p className="text-sm text-muted-foreground">Pair this phone</p>
          </div>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="pair-pin">PIN</Label>
          <Input
            id="pair-pin"
            inputMode="numeric"
            autoComplete="one-time-code"
            pattern="\d{6}"
            maxLength={6}
            placeholder="6 digits"
            value={pin}
            onChange={(event) => setPin(event.target.value.replace(/\D/g, "").slice(0, 6))}
            className="h-12 text-center font-mono text-2xl tracking-[0.4em]"
            autoFocus
          />
          <p className="text-xs text-muted-foreground">
            Find it on the Mac in Solar Hub → Settings → Phone access.
          </p>
          {isAndroidApp && mac ? (
            <p className="text-xs text-muted-foreground">
              Mac at {mac.replace(/^https?:\/\//, "")} ·{" "}
              <button type="button" className="underline" onClick={() => setMac(null)}>
                change
              </button>
            </p>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <Button type="submit" className="w-full" size="lg" disabled={pending || pin.length !== 6}>
          {pending ? "Pairing…" : "Pair"}
        </Button>
      </form>
    </div>
  );
}

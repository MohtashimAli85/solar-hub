import { Sun } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input, Label } from "@/components/ui/input";
import { pairPhone } from "@/lib/transport";

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

  return (
    <div className="flex min-h-dvh items-center justify-center bg-background p-4">
      <form onSubmit={onSubmit} className="w-full max-w-sm space-y-5 rounded-xl border border-border bg-card p-6">
        <div className="flex items-center gap-3">
          <span className="flex size-10 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <Sun className="size-5" />
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

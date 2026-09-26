import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input, Label, Select } from "@/components/ui/input";
import { useEffect, useState } from "react";
import { getSolarSettings, updateSolarSettings } from "@/lib/tauri";
import { TZ_PRESETS } from "@/lib/types";
import type { AppSettings } from "@/lib/types";
import { Save } from "lucide-react";

interface FormState {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  password: string;
}

export function SettingsPage() {
  const [form, setForm] = useState<FormState | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSolarSettings()
      .then((settings: AppSettings) =>
        setForm({
          user_id: settings.user_id,
          station_id: settings.station_id,
          device_id: settings.device_id,
          time_zone: settings.time_zone || "Asia/Karachi",
          password: "",
        }),
      )
      .catch((caught) => setError(String(caught)));
  }, []);

  if (!form) {
    return <p className="text-sm text-muted-foreground">Loading settings…</p>;
  }

  const set = (patch: Partial<FormState>) => setForm((previous) => ({ ...previous!, ...patch }));

  const save = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      await updateSolarSettings({
        user_id: form.user_id,
        station_id: form.station_id,
        device_id: form.device_id,
        time_zone: form.time_zone,
        password: form.password || undefined,
      });
      setMessage("Saved. Inverter now uses the new credentials.");
      setForm((previous) => ({ ...previous!, password: "" }));
    } catch (caught) {
      setError(`Could not save: ${String(caught)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground">Solar cloud credentials and saved Bluetooth device.</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Solar of Things</CardTitle>
          <CardDescription>
            Account used to reach the inverter cloud. The password is stored in the macOS keychain,
            never on disk.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>User ID (email)</Label>
              <Input value={form.user_id} onChange={(event) => set({ user_id: event.target.value })} />
            </div>
            <div className="space-y-1.5">
              <Label>Password</Label>
              <Input
                type="password"
                placeholder="Leave blank to keep the saved one"
                value={form.password}
                onChange={(event) => set({ password: event.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Station ID</Label>
              <Input value={form.station_id} onChange={(event) => set({ station_id: event.target.value })} />
            </div>
            <div className="space-y-1.5">
              <Label>Device ID</Label>
              <Input value={form.device_id} onChange={(event) => set({ device_id: event.target.value })} />
            </div>
            <div className="space-y-1.5">
              <Label>Time zone</Label>
              <Select value={form.time_zone} onChange={(event) => set({ time_zone: event.target.value })}>
                {TZ_PRESETS.map((timeZone) => (
                  <option key={timeZone} value={timeZone}>
                    {timeZone}
                  </option>
                ))}
              </Select>
            </div>
          </div>
          <Button onClick={save} disabled={saving}>
            <Save /> {saving ? "Saving…" : "Save"}
          </Button>
          {message ? <p className="text-sm text-emerald-600 dark:text-emerald-400">{message}</p> : null}
          {error ? <p className="text-sm text-destructive">{error}</p> : null}
        </CardContent>
      </Card>
    </div>
  );
}
import { PhoneAccessCard } from "@/components/settings/PhoneAccessCard";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input, Label, Select } from "@/components/ui/input";
import { useEffect, useState } from "react";
import { getSolarSettings, updateSolarSettings } from "@/lib/tauri";
import { isDesktop } from "@/lib/transport";
import { TZ_PRESETS } from "@/lib/types";
import type { AppSettings } from "@/lib/types";
import { Save } from "lucide-react";

interface FormState {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  password: string;
  location: string;
  gemini_api_key: string;
  has_gemini_api_key: boolean;
  groq_api_key: string;
  has_groq_api_key: boolean;
}

function parseLocation(value: string): { latitude: number | null; longitude: number | null } {
  const parts = value.split(",").map((part) => part.trim());
  if (parts.length !== 2) return { latitude: null, longitude: null };
  const [latitude, longitude] = parts.map(Number);
  if (Number.isNaN(latitude) || Number.isNaN(longitude)) return { latitude: null, longitude: null };
  return { latitude, longitude };
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
          location:
            settings.latitude != null && settings.longitude != null
              ? `${settings.latitude}, ${settings.longitude}`
              : "",
          gemini_api_key: "",
          has_gemini_api_key: settings.has_gemini_api_key,
          groq_api_key: "",
          has_groq_api_key: settings.has_groq_api_key,
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
      const { latitude, longitude } = parseLocation(form.location);
      const settings = await updateSolarSettings({
        user_id: form.user_id,
        station_id: form.station_id,
        device_id: form.device_id,
        time_zone: form.time_zone,
        password: form.password || undefined,
        latitude,
        longitude,
        gemini_api_key: form.gemini_api_key || undefined,
        groq_api_key: form.groq_api_key || undefined,
      });
      setMessage("Saved. Inverter now uses the new credentials.");
      setForm((previous) => ({
        ...previous!,
        password: "",
        gemini_api_key: "",
        has_gemini_api_key: settings.has_gemini_api_key,
        groq_api_key: "",
        has_groq_api_key: settings.has_groq_api_key,
      }));
    } catch (caught) {
      setError(`Could not save: ${String(caught)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-bold tracking-tight sm:text-2xl">Settings</h1>
        <p className="text-sm text-muted-foreground sm:text-base">Solar cloud credentials and phone access.</p>
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
          {isDesktop ? null : (
            <p className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
              These can only be changed in the desktop app.
            </p>
          )}
          <fieldset disabled={!isDesktop} className="grid gap-4 disabled:opacity-70 sm:grid-cols-2">
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
            <div className="space-y-1.5">
              <Label>Location (lat, long)</Label>
              <Input
                placeholder="Enter latitude and longitude separated by a comma"
                value={form.location}
                onChange={(event) => set({ location: event.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Gemini API key</Label>
              <Input
                type="password"
                placeholder={form.has_gemini_api_key ? "Saved — leave blank to keep" : "For the automation agent"}
                value={form.gemini_api_key}
                onChange={(event) => set({ gemini_api_key: event.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Groq API key (optional)</Label>
              <Input
                type="password"
                placeholder={form.has_groq_api_key ? "Saved — leave blank to keep" : "Backup when Gemini is busy"}
                value={form.groq_api_key}
                onChange={(event) => set({ groq_api_key: event.target.value })}
              />
            </div>
          </fieldset>
          {isDesktop ? (
            <Button onClick={save} disabled={saving}>
              <Save /> {saving ? "Saving…" : "Save"}
            </Button>
          ) : null}
          {message ? <p className="text-sm text-emerald-600 dark:text-emerald-400">{message}</p> : null}
          {error ? <p className="text-sm text-destructive">{error}</p> : null}
        </CardContent>
      </Card>

      {isDesktop ? <PhoneAccessCard /> : null}
    </div>
  );
}
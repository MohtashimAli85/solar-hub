import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

export type { UnlistenFn };

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const isAndroidApp = isTauri && /Android/i.test(navigator.userAgent);
export const isDesktop = isTauri && !isAndroidApp;

const TOKEN_KEY = "solar-hub-phone-token";
const MAC_KEY = "solar-hub-mac-address";
const OFFLINE_KEY = "solar-hub-mac-offline-since";
const CACHE_PREFIX = "solar-hub-cache:";
const RETRY_MS = 3_000;
const REQUEST_TIMEOUT_MS = 10_000;
const PROBE_MS = 15_000;
const FULL_SCAN_EVERY = 8;

const LOCAL_COMMANDS = new Set([
  "scan_bms_devices",
  "get_battery_state",
  "get_battery_connection",
  "connect_bms_device",
  "disconnect_bms_device",
  "get_saved_bms_device",
  "set_saved_bms_device",
  "reconnect_saved_bms",
  "get_inverter_snapshot",
  "get_inverter_settings",
  "set_output_priority",
  "set_charger_priority",
  "set_ac_input_range",
  "set_grid_feed_in",
  "set_smart_load",
  "set_low_battery_cutoff_voltage",
  "set_high_cutoff_voltage",
  "set_low_dc_cutoff_soc",
  "set_max_total_charge_current",
  "set_max_utility_charge_current",
  "get_device_details",
  "get_solar_settings",
]);

type Handler = (event: { payload: unknown }) => void;

export type Link = { mode: "mac" | "direct" | "starting"; offlineSince: string | null };

const handlers = new Map<string, Set<Handler>>();
const attached = new Set<string>();
const localUnlisteners = new Map<string, Promise<UnlistenFn>>();
const pairingListeners = new Set<() => void>();
const linkListeners = new Set<(link: Link) => void>();
let source: EventSource | null = null;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
let probeTimer: ReturnType<typeof setTimeout> | undefined;
let probeCount = 0;
let link: Link = { mode: isAndroidApp ? "starting" : "mac", offlineSince: read(OFFLINE_KEY) };

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
  } catch {
    // storage unavailable; the phone will have to pair again next visit
  }
}

function macBase(): string {
  return isAndroidApp ? (read(MAC_KEY) ?? "") : "";
}

export function hasPhoneToken(): boolean {
  return read(TOKEN_KEY) != null;
}

export function onPairingNeeded(listener: () => void): () => void {
  pairingListeners.add(listener);
  return () => pairingListeners.delete(listener);
}

export function getLink(): Link {
  return link;
}

export function onLinkChange(listener: (link: Link) => void): () => void {
  linkListeners.add(listener);
  return () => linkListeners.delete(listener);
}

function setLink(next: Link) {
  link = next;
  write(OFFLINE_KEY, next.offlineSince);
  linkListeners.forEach((listener) => listener(next));
}

function authHeaders(): Record<string, string> {
  const token = read(TOKEN_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

function closeEvents() {
  clearTimeout(retryTimer);
  source?.close();
  source = null;
  attached.clear();
}

function requirePairing() {
  write(TOKEN_KEY, null);
  closeEvents();
  pairingListeners.forEach((listener) => listener());
}

async function fetchMac(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`${macBase()}${path}`, { ...init, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
}

export function unpairPhone() {
  clearTimeout(probeTimer);
  write(MAC_KEY, null);
  requirePairing();
}

export async function checkPhoneSession(): Promise<boolean> {
  const response = await fetchMac("/api/session", { headers: authHeaders() });
  if (response.status === 401) {
    requirePairing();
    return false;
  }
  return response.ok;
}

export async function findMac(): Promise<string | null> {
  const found = await tauriInvoke<string | null>("find_mac", { preferred: read(MAC_KEY) });
  if (found) write(MAC_KEY, found);
  return found;
}

export function setMacAddress(address: string) {
  let value = address.trim();
  if (!/^https?:\/\//.test(value)) value = `http://${value}`;
  const url = new URL(value);
  if (!url.port) url.port = "8787";
  write(MAC_KEY, url.origin);
}

export function getMacAddress(): string | null {
  return read(MAC_KEY);
}

async function importMacSetup() {
  const setup = await httpInvoke<unknown>("get_phone_setup");
  await tauriInvoke("import_mac_setup", { setup });
}

export async function pairPhone(pin: string): Promise<void> {
  const response = await fetchMac("/api/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pin }),
  });
  if (!response.ok) {
    throw new Error((await response.text()) || `Pairing failed (${response.status}).`);
  }
  const { token } = (await response.json()) as { token: string };
  write(TOKEN_KEY, token);
  if (isAndroidApp) {
    await switchToMac();
  } else {
    openEvents();
  }
}

function cacheKey(command: string, args?: Record<string, unknown>) {
  return `${CACHE_PREFIX}${command}:${JSON.stringify(args ?? {})}`;
}

function readCache<T>(command: string, args?: Record<string, unknown>): T | undefined {
  const raw = read(cacheKey(command, args));
  if (raw == null) return undefined;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return undefined;
  }
}

async function httpInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const response = await fetchMac(`/api/invoke/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...authHeaders() },
    body: JSON.stringify(args ?? {}),
  });
  if (response.status === 401) {
    requirePairing();
  }
  const text = await response.text();
  if (!response.ok) {
    throw text || `Request failed (${response.status}).`;
  }
  if (isAndroidApp && command.startsWith("get_") && command !== "get_phone_setup") {
    write(cacheKey(command, args), text || "null");
  }
  return (text ? JSON.parse(text) : null) as T;
}

function isNetworkError(error: unknown): boolean {
  return error instanceof TypeError || (error instanceof DOMException && error.name === "TimeoutError");
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isDesktop) return tauriInvoke<T>(command, args);
  if (!isAndroidApp) return httpInvoke<T>(command, args);

  if (link.mode !== "direct") {
    try {
      return await httpInvoke<T>(command, args);
    } catch (error) {
      if (!isNetworkError(error)) throw error;
      void switchToDirect();
    }
  }
  if (LOCAL_COMMANDS.has(command)) return tauriInvoke<T>(command, args);
  const cached = readCache<T>(command, args);
  if (cached !== undefined) return cached;
  throw "The Mac is offline, so this isn't available right now.";
}

function attach(name: string) {
  if (!source || attached.has(name)) return;
  attached.add(name);
  source.addEventListener(name, (event) => {
    const payload: unknown = JSON.parse((event as MessageEvent<string>).data);
    handlers.get(name)?.forEach((handler) => handler({ payload }));
  });
}

function attachLocal(name: string) {
  if (localUnlisteners.has(name)) return;
  localUnlisteners.set(
    name,
    tauriListen(name, (event) => handlers.get(name)?.forEach((handler) => handler({ payload: event.payload }))),
  );
}

function detachLocal() {
  localUnlisteners.forEach((unlisten) => void unlisten.then((fn) => fn()));
  localUnlisteners.clear();
}

function openEvents() {
  if (source || handlers.size === 0) return;
  if (isAndroidApp && link.mode !== "mac") return;
  const token = read(TOKEN_KEY);
  if (!token) return;
  source = new EventSource(`${macBase()}/api/events?token=${encodeURIComponent(token)}`);
  handlers.forEach((_, name) => attach(name));
  source.onerror = () => {
    if (source?.readyState !== EventSource.CLOSED) return;
    closeEvents();
    retryTimer = setTimeout(() => {
      void checkPhoneSession()
        .then((ok) => ok && openEvents())
        .catch((error: unknown) => {
          if (isAndroidApp && isNetworkError(error)) void switchToDirect();
          else openEvents();
        });
    }, RETRY_MS);
  };
}

async function switchToMac() {
  clearTimeout(probeTimer);
  detachLocal();
  await tauriInvoke("set_direct_mode", { on: false }).catch(() => undefined);
  setLink({ mode: "mac", offlineSince: null });
  openEvents();
  void importMacSetup().catch(() => undefined);
}

async function switchToDirect() {
  if (link.mode === "direct") return;
  closeEvents();
  setLink({ mode: "direct", offlineSince: link.offlineSince ?? new Date().toISOString() });
  handlers.forEach((_, name) => attachLocal(name));
  await tauriInvoke("set_direct_mode", { on: true }).catch(() => undefined);
  scheduleProbe();
}

function scheduleProbe() {
  clearTimeout(probeTimer);
  probeTimer = setTimeout(() => void probeMac(), PROBE_MS);
}

async function probeMac() {
  if (link.mode !== "direct") return;
  probeCount += 1;
  try {
    if (await checkPhoneSession()) return void (await switchToMac());
  } catch {
    // Mac still unreachable at the last address
  }
  if (probeCount % FULL_SCAN_EVERY === 0 && (await findMac().catch(() => null))) {
    try {
      if (await checkPhoneSession()) return void (await switchToMac());
    } catch {
      // found but not answering yet
    }
  }
  scheduleProbe();
}

export async function startAndroidLink(): Promise<void> {
  if (!isAndroidApp || !hasPhoneToken()) return;
  if (link.mode === "direct") {
    link = { ...link, mode: "starting" };
  }
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      if (await checkPhoneSession()) return void (await switchToMac());
      if (!hasPhoneToken()) return;
    } catch {
      if (attempt === 0 && (await findMac().catch(() => null))) continue;
    }
    break;
  }
  await switchToDirect();
}

export async function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<UnlistenFn> {
  if (isDesktop) return tauriListen<T>(event, handler);
  const typed = handler as Handler;
  let set = handlers.get(event);
  if (!set) {
    set = new Set();
    handlers.set(event, set);
  }
  set.add(typed);
  if (isAndroidApp && link.mode === "direct") attachLocal(event);
  attach(event);
  openEvents();
  return () => {
    set.delete(typed);
    if (set.size === 0) handlers.delete(event);
    if (handlers.size === 0) closeEvents();
  };
}

export function openExternal(url: string) {
  if (isTauri) {
    void openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

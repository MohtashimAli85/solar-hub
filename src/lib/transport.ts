import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

export type { UnlistenFn };

export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const TOKEN_KEY = "solar-hub-phone-token";
const RETRY_MS = 3_000;

type Handler = (event: { payload: unknown }) => void;

const handlers = new Map<string, Set<Handler>>();
const attached = new Set<string>();
const pairingListeners = new Set<() => void>();
let source: EventSource | null = null;
let retryTimer: ReturnType<typeof setTimeout> | undefined;

function readToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

function writeToken(token: string | null) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    // storage unavailable; the phone will have to pair again next visit
  }
}

export function hasPhoneToken(): boolean {
  return readToken() != null;
}

export function onPairingNeeded(listener: () => void): () => void {
  pairingListeners.add(listener);
  return () => pairingListeners.delete(listener);
}

function authHeaders(): Record<string, string> {
  const token = readToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

function closeEvents() {
  clearTimeout(retryTimer);
  source?.close();
  source = null;
  attached.clear();
}

function requirePairing() {
  writeToken(null);
  closeEvents();
  pairingListeners.forEach((listener) => listener());
}

export async function checkPhoneSession(): Promise<boolean> {
  const response = await fetch("/api/session", { headers: authHeaders() });
  if (response.status === 401) {
    requirePairing();
    return false;
  }
  return response.ok;
}

export async function pairPhone(pin: string): Promise<void> {
  const response = await fetch("/api/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pin }),
  });
  if (!response.ok) {
    throw new Error((await response.text()) || `Pairing failed (${response.status}).`);
  }
  const { token } = (await response.json()) as { token: string };
  writeToken(token);
  openEvents();
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isDesktop) return tauriInvoke<T>(command, args);
  const response = await fetch(`/api/invoke/${command}`, {
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
  return (text ? JSON.parse(text) : null) as T;
}

function attach(name: string) {
  if (!source || attached.has(name)) return;
  attached.add(name);
  source.addEventListener(name, (event) => {
    const payload: unknown = JSON.parse((event as MessageEvent<string>).data);
    handlers.get(name)?.forEach((handler) => handler({ payload }));
  });
}

function openEvents() {
  if (source || handlers.size === 0) return;
  const token = readToken();
  if (!token) return;
  source = new EventSource(`/api/events?token=${encodeURIComponent(token)}`);
  handlers.forEach((_, name) => attach(name));
  source.onerror = () => {
    if (source?.readyState !== EventSource.CLOSED) return;
    closeEvents();
    retryTimer = setTimeout(() => {
      void checkPhoneSession()
        .then((ok) => ok && openEvents())
        .catch(() => openEvents());
    }, RETRY_MS);
  };
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
  attach(event);
  openEvents();
  return () => {
    set.delete(typed);
    if (set.size === 0) handlers.delete(event);
    if (handlers.size === 0) closeEvents();
  };
}

export function openExternal(url: string) {
  if (isDesktop) {
    void openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

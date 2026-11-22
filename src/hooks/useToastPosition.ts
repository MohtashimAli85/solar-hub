import { useCallback, useSyncExternalStore } from "react";

export type ToastPosition = "top-right" | "bottom-right";

const STORAGE_KEY = "solar-hub-toast-position";
const CHANGED = "solar-hub-toast-position-changed";

function read(): ToastPosition {
  try {
    return localStorage.getItem(STORAGE_KEY) === "top-right" ? "top-right" : "bottom-right";
  } catch {
    return "bottom-right";
  }
}

function subscribe(onChange: () => void) {
  window.addEventListener(CHANGED, onChange);
  window.addEventListener("storage", onChange);
  return () => {
    window.removeEventListener(CHANGED, onChange);
    window.removeEventListener("storage", onChange);
  };
}

/** Where the "do this now" toasts sit; kept on this device. */
export function useToastPosition() {
  const position = useSyncExternalStore(subscribe, read, read);
  const setPosition = useCallback((next: ToastPosition) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      return;
    }
    window.dispatchEvent(new Event(CHANGED));
  }, []);
  return { position, setPosition };
}

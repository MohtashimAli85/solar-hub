import { useSyncExternalStore } from "react";
import { getLink, onLinkChange } from "@/lib/transport";

export function useLink() {
  return useSyncExternalStore(onLinkChange, getLink);
}

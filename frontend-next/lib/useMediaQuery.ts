"use client";

/**
 * A tiny media query hook.
 *
 * The layout uses this instead of CSS breakpoints for the parts that change
 * behaviour rather than appearance - the sidebar becomes a drawer, the
 * network panel collapses into a sheet.
 */

import { useEffect, useState } from "react";

/** Subscribe to a media query. */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(false);

  useEffect(() => {
    const list = window.matchMedia(query);
    setMatches(list.matches);
    const onChange = (event: MediaQueryListEvent) => setMatches(event.matches);
    list.addEventListener("change", onChange);
    return () => list.removeEventListener("change", onChange);
  }, [query]);

  return matches;
}

/** True on narrow screens (phones and small tablets). */
export function useIsCompact(): boolean {
  return useMediaQuery("(max-width: 860px)");
}

/** True on very narrow screens (phones). */
export function useIsPhone(): boolean {
  return useMediaQuery("(max-width: 620px)");
}

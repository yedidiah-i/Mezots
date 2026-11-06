"use client";

/**
 * The theme.
 *
 * Three modes: light, dark, and follow the operating system. The choice is
 * remembered locally. The dark palette is the default because a school
 * network is usually dimmed during lessons.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/** The available theme modes. */
export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "mezots.theme";

/** What the theme context exposes. */
interface ThemeContextValue {
  mode: ThemeMode;
  /** The theme actually in effect. */
  resolved: "light" | "dark";
  setMode: (mode: ThemeMode) => void;
  /** Flip between light and dark. */
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/** Read the theme context. */
export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used inside ThemeProvider");
  return value;
}

/** Provide the theme to the tree. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [mode, setModeState] = useState<ThemeMode>("dark");
  const [systemDark, setSystemDark] = useState(true);

  // Load the saved mode once the component mounts, because the server cannot
  // know what the visitor prefers.
  useEffect(() => {
    try {
      const stored = window.localStorage.getItem(STORAGE_KEY) as ThemeMode | null;
      if (stored === "light" || stored === "dark" || stored === "system") {
        setModeState(stored);
      }
    } catch {
      /* storage unavailable */
    }

    const query = window.matchMedia("(prefers-color-scheme: dark)");
    setSystemDark(query.matches);
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const resolved: "light" | "dark" =
    mode === "system" ? (systemDark ? "dark" : "light") : mode;

  // Apply the theme to the document so the CSS variables switch.
  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", resolved === "dark");
    root.style.colorScheme = resolved;
  }, [resolved]);

  const setMode = useCallback((next: ThemeMode) => {
    setModeState(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* storage unavailable */
    }
  }, []);

  const toggle = useCallback(() => {
    setMode(resolved === "dark" ? "light" : "dark");
  }, [resolved, setMode]);

  const value = useMemo(
    () => ({ mode, resolved, setMode, toggle }),
    [mode, resolved, setMode, toggle],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

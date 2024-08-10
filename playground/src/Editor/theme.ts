/**
 * Light and dark mode theming.
 */
import { useState } from "react";

export type Theme = "dark" | "light";

export function useTheme(): [Theme, (theme: Theme) => void] {
  const [localTheme, setLocalTheme] = useState<Theme>(() =>
    detectInitialTheme(),
  );

  const setTheme = (mode: Theme) => {
    if (mode === "dark") {
      document.body.classList.add("dark");
    } else {
      document.body.classList.remove("dark");
    }
    localStorage.setItem("theme", mode);
    setLocalTheme(mode);
  };

  return [localTheme, setTheme];
}

function detectInitialTheme(): Theme {
  const initialTheme = localStorage.getItem("theme");
  if (initialTheme === "dark") {
    return "dark";
  } else if (initialTheme === "light") {
    return "light";
  } else if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
    return "dark";
  } else {
    return "light";
  }
}

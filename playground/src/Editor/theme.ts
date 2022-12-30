/**
 * Light and dark mode theming.
 */
import { useEffect, useState } from "react";

export type Theme = "dark" | "light";

export function useTheme(): [Theme, (theme: Theme) => void] {
  const [localTheme, setLocalTheme] = useState<Theme>("light");

  const setTheme = (mode: Theme) => {
    if (mode === "dark") {
      document.body.classList.add("dark");
    } else {
      document.body.classList.remove("dark");
    }
    localStorage.setItem("theme", mode);
    setLocalTheme(mode);
  };

  useEffect(() => {
    const initialTheme = localStorage.getItem("theme");
    if (initialTheme === "dark") {
      setTheme("dark");
    } else if (initialTheme === "light") {
      setTheme("light");
    } else if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
      setTheme("dark");
    } else {
      setTheme("light");
    }
  }, []);

  return [localTheme, setTheme];
}

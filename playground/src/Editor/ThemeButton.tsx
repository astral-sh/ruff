/**
 * Button to toggle between light and dark mode themes.
 */
import { Theme } from "./theme";

export default function ThemeButton({
  theme,
  onChange,
}: {
  theme: Theme;
  onChange: (theme: Theme) => void;
}) {
  return (
    <button
      type="button"
      className="ml-4 sm:ml-0 ring-1 ring-gray-900/5 shadow-sm hover:bg-gray-50 dark:ring-0 dark:bg-gray-800 dark:hover:bg-gray-700 dark:shadow-highlight/4 group focus:outline-none focus-visible:ring-2 rounded-md focus-visible:ring-ayu-accent dark:focus-visible:ring-2 dark:focus-visible:ring-gray-400"
      onClick={() => onChange(theme === "light" ? "dark" : "light")}
    >
      <span className="sr-only">
        <span className="dark:hidden">Switch to dark theme</span>
        <span className="hidden dark:inline">Switch to light theme</span>
      </span>
      <svg
        width="36"
        height="36"
        viewBox="-6 -6 36 36"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="stroke-ayu-accent fill-ayu-accent/10 group-hover:stroke-ayu-accent/80 dark:stroke-gray-400 dark:fill-gray-400/20 dark:group-hover:stroke-gray-300"
      >
        <g className="dark:opacity-0">
          <path d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"></path>
          <path
            d="M12 4v.01M17.66 6.345l-.007.007M20.005 12.005h-.01M17.66 17.665l-.007-.007M12 20.01V20M6.34 17.665l.007-.007M3.995 12.005h.01M6.34 6.344l.007.007"
            fill="none"
          />
        </g>
        <g className="opacity-0 dark:opacity-100">
          <path d="M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z" />
          <path
            d="M12 3v1M18.66 5.345l-.828.828M21.005 12.005h-1M18.66 18.665l-.828-.828M12 21.01V20M5.34 18.666l.835-.836M2.995 12.005h1.01M5.34 5.344l.835.836"
            fill="none"
          />
        </g>
      </svg>
    </button>
  );
}

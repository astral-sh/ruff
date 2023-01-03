import classNames from "classnames";
import RepoButton from "./RepoButton";
import ThemeButton from "./ThemeButton";
import ShareButton from "./ShareButton";
import { Theme } from "./theme";
import VersionTag from "./VersionTag";

export type Tab = "Source" | "Settings";

export default function Header({
  edit,
  tab,
  theme,
  version,
  onChangeTab,
  onChangeTheme,
  onShare,
}: {
  edit: number;
  tab: Tab;
  theme: Theme;
  version: string | null;
  onChangeTab: (tab: Tab) => void;
  onChangeTheme: (theme: Theme) => void;
  onShare?: () => void;
}) {
  return (
    <div
      className={classNames(
        "w-full",
        "flex",
        "items-center",
        "justify-between",
        "flex-none",
        "pl-5",
        "sm:pl-6",
        "pr-4",
        "lg:pr-6",
        "absolute",
        "z-10",
        "top-0",
        "left-0",
        "-mb-px",
        "antialiased",
        "border-b",
        "border-gray-200",
        "dark:border-gray-800",
      )}
    >
      <div className="flex space-x-5">
        <button
          type="button"
          className={classNames(
            "relative flex py-3 text-sm leading-6 font-semibold focus:outline-none",
            tab === "Source"
              ? "text-ayu-accent"
              : "text-gray-700 hover:text-gray-900 focus:text-gray-900 dark:text-gray-300 dark:hover:text-white",
          )}
          onClick={() => onChangeTab("Source")}
        >
          <span
            className={classNames(
              "absolute bottom-0 inset-x-0 bg-ayu-accent h-0.5 rounded-full transition-opacity duration-150",
              tab === "Source" ? "opacity-100" : "opacity-0",
            )}
          />
          Source
        </button>
        <button
          type="button"
          className={classNames(
            "relative flex py-3 text-sm leading-6 font-semibold focus:outline-none",
            tab === "Settings"
              ? "text-ayu-accent"
              : "text-gray-700 hover:text-gray-900 focus:text-gray-900 dark:text-gray-300 dark:hover:text-white",
          )}
          onClick={() => onChangeTab("Settings")}
        >
          <span
            className={classNames(
              "absolute bottom-0 inset-x-0 bg-ayu-accent h-0.5 rounded-full transition-opacity duration-150",
              tab === "Settings" ? "opacity-100" : "opacity-0",
            )}
          />
          Settings
        </button>
      </div>
      <div className={"flex items-center min-w-0"}>
        {version ? (
          <div className={"hidden sm:flex items-center"}>
            <VersionTag>v{version}</VersionTag>
          </div>
        ) : null}
        <div className="hidden sm:block mx-6 lg:mx-4 w-px h-6 bg-gray-200 dark:bg-gray-700" />
        <RepoButton />
        <div className="hidden sm:block mx-6 lg:mx-4 w-px h-6 bg-gray-200 dark:bg-gray-700" />
        <div className="hidden sm:block">
          <ShareButton key={edit} onShare={onShare} />
        </div>
        <div className="hidden sm:block mx-6 lg:mx-4 w-px h-6 bg-gray-200 dark:bg-gray-700" />
        <ThemeButton theme={theme} onChange={onChangeTheme} />
      </div>
    </div>
  );
}

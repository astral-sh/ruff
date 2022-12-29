import classNames from "classnames";
import ShareButton from "./ShareButton";
import VersionTag from "./VersionTag";

export type Tab = "Source" | "Settings";

export default function Header({
  edit,
  version,
  tab,
  onChange,
  onShare,
}: {
  edit: number;
  version: string | null;
  tab: Tab;
  onChange: (tab: Tab) => void;
  onShare?: () => void;
}) {
  return (
    <div
      className="w-full flex items-center justify-between flex-none pl-5 sm:pl-6 pr-4 lg:pr-6 absolute z-10 top-0 left-0 -mb-px antialiased border-b border-gray-200 dark:border-gray-800"
      style={{ background: "#f8f9fa" }}
    >
      <div className="flex space-x-5">
        <button
          type="button"
          className={classNames(
            "relative flex py-3 text-sm leading-6 font-semibold focus:outline-none",
            tab === "Source"
              ? "text-ayu"
              : "text-gray-700 hover:text-gray-900 focus:text-gray-900 dark:text-gray-300 dark:hover:text-white"
          )}
          onClick={() => onChange("Source")}
        >
          <span
            className={classNames(
              "absolute bottom-0 inset-x-0 bg-ayu h-0.5 rounded-full transition-opacity duration-150",
              tab === "Source" ? "opacity-100" : "opacity-0"
            )}
          />
          Source
        </button>
        <button
          type="button"
          className={classNames(
            "relative flex py-3 text-sm leading-6 font-semibold focus:outline-none",
            tab === "Settings"
              ? "text-ayu"
              : "text-gray-700 hover:text-gray-900 focus:text-gray-900 dark:text-gray-300 dark:hover:text-white"
          )}
          onClick={() => onChange("Settings")}
        >
          <span
            className={classNames(
              "absolute bottom-0 inset-x-0 bg-ayu h-0.5 rounded-full transition-opacity duration-150",
              tab === "Settings" ? "opacity-100" : "opacity-0"
            )}
          />
          Settings
        </button>
        {version ? (
          <div className={"flex items-center"}>
            <VersionTag>v{version}</VersionTag>
          </div>
        ) : null}
      </div>
      <div className={"hidden sm:flex items-center min-w-0"}>
        <ShareButton key={edit} onShare={onShare} />
      </div>
    </div>
  );
}

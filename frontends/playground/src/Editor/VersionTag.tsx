import classNames from "classnames";
import { ReactNode } from "react";

export default function VersionTag({ children }: { children: ReactNode }) {
  return (
    <div
      className={classNames(
        "text-gray-500",
        "text-xs",
        "leading-5",
        "font-semibold",
        "bg-gray-400/10",
        "rounded-full",
        "py-1",
        "px-3",
        "flex",
        "items-center",
        "dark:bg-gray-800",
        "dark:text-gray-400",
        "dark:shadow-highlight/4",
      )}
    >
      {children}
    </div>
  );
}

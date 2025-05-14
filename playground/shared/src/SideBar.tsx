import { type ReactNode } from "react";
import classNames from "classnames";

type SideBarProps = {
  children: ReactNode;
  position: "right" | "left";
};

export default function SideBar({ children, position }: SideBarProps) {
  return (
    <ul
      className={classNames(
        "w-12 flex-initial  flex flex-col items-stretch bg-galaxy border-gray-200",
        position === "left" ? "border-r" : "border-l",
      )}
    >
      {children}
    </ul>
  );
}

export interface SideBarEntryProps {
  title: string;
  selected: boolean;
  children: ReactNode;
  position: "left" | "right";

  onClick?(): void;
}

export function SideBarEntry({
  title,
  onClick,
  children,
  selected,
  position,
}: SideBarEntryProps) {
  return (
    <li
      aria-label={title}
      onClick={onClick}
      role="button"
      className={`group py-4 px-2 relative flex items-center justify-center flex-col fill-white text-white cursor-pointer ${
        selected ? "opacity-100" : "opacity-50 hover:opacity-100"
      }`}
    >
      {children}
      {selected && (
        <span className="absolute start-0 inset-y-0 bg-white w-0.5"></span>
      )}

      <Tooltip position={position}>{title}</Tooltip>
    </li>
  );
}

interface TooltipProps {
  children: ReactNode;
  position: "left" | "right";
}

function Tooltip({ children, position }: TooltipProps) {
  return (
    <span
      className={`z-10 absolute rounded dark:border-[1px] dark:border-white bg-space dark:bg-white px-2 py-1 hidden text-xs text-white dark:text-black group-hover:flex whitespace-nowrap ${
        position === "right" ? "right-[52px]" : "left-[52px]"
      }`}
    >
      {children}
    </span>
  );
}

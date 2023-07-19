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
        "w-12 flex-initial  flex flex-col items-stretch bg-galaxy",
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
  onClick?(): void;
}

export function SideBarEntry({
  title,
  onClick,
  children,
  selected,
}: SideBarEntryProps) {
  return (
    <li
      title={title}
      onClick={onClick}
      role="button"
      className={`py-4 px-2 relative flex items-center flex-col ${
        selected ? "fill-white text-white" : "fill-slate-500 text-slate-500"
      }`}
    >
      {children}
      {selected && (
        <span className="absolute start-0 inset-y-0 bg-white w-0.5"></span>
      )}
    </li>
  );
}

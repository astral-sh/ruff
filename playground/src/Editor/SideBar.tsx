import { FileIcon, SettingsIcon } from "./Icons";
import { type ReactNode } from "react";

type Tool = "Settings" | "Source";

type SideBarProps = {
  selected: Tool;
  onSelectTool(tool: Tool): void;
};

export default function SideBar({ selected, onSelectTool }: SideBarProps) {
  return (
    <ul className="w-12 flex-initial border-r bg-galaxy flex flex-col items-stretch">
      <SideBarEntry
        title="Source"
        onClick={() => onSelectTool("Source")}
        selected={selected == "Source"}
      >
        <FileIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Settings"
        onClick={() => onSelectTool("Settings")}
        selected={selected == "Settings"}
      >
        <SettingsIcon />
      </SideBarEntry>
    </ul>
  );
}

interface SideBarEntryProps {
  title: string;
  selected: boolean;
  onClick?(): void;
  children: ReactNode;
}

function SideBarEntry({
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
        selected ? "fill-white" : "fill-slate-500"
      }`}
    >
      {children}
      {selected && (
        <span className="absolute start-0 inset-y-0 bg-white w-0.5 rounded-full transition-opacity duration-150 opacity-100"></span>
      )}
    </li>
  );
}

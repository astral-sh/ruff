import { FileIcon, SettingsIcon } from "./Icons";
import SideBar, { SideBarEntry } from "./SideBar";

type Tool = "Settings" | "Source";

type SideBarProps = {
  selected: Tool;
  onSelectTool(tool: Tool): void;
};

export default function PrimarySideBar({
  selected,
  onSelectTool,
}: SideBarProps) {
  return (
    <SideBar position="left">
      <SideBarEntry
        title="Source"
        position={"left"}
        onClick={() => onSelectTool("Source")}
        selected={selected == "Source"}
      >
        <FileIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Settings"
        position={"left"}
        onClick={() => onSelectTool("Settings")}
        selected={selected == "Settings"}
      >
        <SettingsIcon />
      </SideBarEntry>
    </SideBar>
  );
}

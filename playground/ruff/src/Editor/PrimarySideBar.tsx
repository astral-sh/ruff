import { Icons, SideBar, SideBarEntry } from "shared";

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
        selected={selected === "Source"}
      >
        <Icons.File />
      </SideBarEntry>

      <SideBarEntry
        title="Settings"
        position={"left"}
        onClick={() => onSelectTool("Settings")}
        selected={selected === "Settings"}
      >
        <Icons.Settings />
      </SideBarEntry>
    </SideBar>
  );
}

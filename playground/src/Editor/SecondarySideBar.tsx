import SideBar, { SideBarEntry } from "./SideBar";
import { FormatIcon, StructureIcon, TokensIcon } from "./Icons";
import { SecondaryTool } from "./SecondaryPanel";

interface RightSideBarProps {
  selected: SecondaryTool | null;
  onSelected(tool: SecondaryTool): void;
}

export default function SecondarySideBar({
  selected,
  onSelected,
}: RightSideBarProps) {
  return (
    <SideBar position="right">
      <SideBarEntry
        title="Format (alpha)"
        selected={selected === "Format"}
        onClick={() => onSelected("Format")}
      >
        <FormatIcon />
      </SideBarEntry>

      <SideBarEntry
        title="AST"
        selected={selected === "AST"}
        onClick={() => onSelected("AST")}
      >
        <StructureIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Tokens"
        selected={selected === "Tokens"}
        onClick={() => onSelected("Tokens")}
      >
        <TokensIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter IR"
        selected={selected === "FIR"}
        onClick={() => onSelected("FIR")}
      >
        FIR
      </SideBarEntry>
    </SideBar>
  );
}

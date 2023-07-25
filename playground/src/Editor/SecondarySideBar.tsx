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
        selected={selected === SecondaryTool.Format}
        onClick={() => onSelected(SecondaryTool.Format)}
      >
        <FormatIcon />
      </SideBarEntry>

      <SideBarEntry
        title="AST"
        selected={selected === SecondaryTool.AST}
        onClick={() => onSelected(SecondaryTool.AST)}
      >
        <StructureIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Tokens"
        selected={selected === SecondaryTool.Tokens}
        onClick={() => onSelected(SecondaryTool.Tokens)}
      >
        <TokensIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter IR"
        selected={selected === SecondaryTool.FIR}
        onClick={() => onSelected(SecondaryTool.FIR)}
      >
        FIR
      </SideBarEntry>
    </SideBar>
  );
}

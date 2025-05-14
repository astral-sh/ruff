import { Icons, SideBar, SideBarEntry } from "shared";
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
        title="Format"
        position={"right"}
        selected={selected === SecondaryTool.Format}
        onClick={() => onSelected(SecondaryTool.Format)}
      >
        <Icons.Format />
      </SideBarEntry>

      <SideBarEntry
        title="AST"
        position={"right"}
        selected={selected === SecondaryTool.AST}
        onClick={() => onSelected(SecondaryTool.AST)}
      >
        <Icons.Structure />
      </SideBarEntry>

      <SideBarEntry
        title="Tokens"
        position={"right"}
        selected={selected === SecondaryTool.Tokens}
        onClick={() => onSelected(SecondaryTool.Tokens)}
      >
        <Icons.Token />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter IR"
        position={"right"}
        selected={selected === SecondaryTool.FIR}
        onClick={() => onSelected(SecondaryTool.FIR)}
      >
        <Icons.FormatterIR />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter comments"
        position={"right"}
        selected={selected === SecondaryTool.Comments}
        onClick={() => onSelected(SecondaryTool.Comments)}
      >
        <Icons.Comments />
      </SideBarEntry>
    </SideBar>
  );
}

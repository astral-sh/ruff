import SideBar, { SideBarEntry } from "./SideBar";
import {
  FormatIcon,
  FormatterIRIcon,
  StructureIcon,
  TokensIcon,
  CommentsIcon,
} from "./Icons";
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
        <FormatIcon />
      </SideBarEntry>

      <SideBarEntry
        title="AST"
        position={"right"}
        selected={selected === SecondaryTool.AST}
        onClick={() => onSelected(SecondaryTool.AST)}
      >
        <StructureIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Tokens"
        position={"right"}
        selected={selected === SecondaryTool.Tokens}
        onClick={() => onSelected(SecondaryTool.Tokens)}
      >
        <TokensIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter IR"
        position={"right"}
        selected={selected === SecondaryTool.FIR}
        onClick={() => onSelected(SecondaryTool.FIR)}
      >
        <FormatterIRIcon />
      </SideBarEntry>

      <SideBarEntry
        title="Formatter comments"
        position={"right"}
        selected={selected === SecondaryTool.Comments}
        onClick={() => onSelected(SecondaryTool.Comments)}
      >
        <CommentsIcon />
      </SideBarEntry>
    </SideBar>
  );
}

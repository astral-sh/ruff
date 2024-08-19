import SideBar, { SideBarEntry } from "../shared/SideBar";
import { StructureIcon, TokensIcon } from "../shared/Icons";
import { SecondaryTool } from "./SecondaryPanel";

interface Props {
  selected: SecondaryTool | null;
  onSelected(tool: SecondaryTool): void;
}

export default function SecondarySideBar({ selected, onSelected }: Props) {
  return (
    <SideBar position="right">
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
    </SideBar>
  );
}

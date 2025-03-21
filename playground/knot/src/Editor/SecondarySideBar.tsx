import { Icons, SideBar, SideBarEntry } from "shared";
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
    </SideBar>
  );
}

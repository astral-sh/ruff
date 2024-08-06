import SideBar from "../shared/SideBar";

export type Props = {
  // The file names
  files: Array<string>;
  // The name of the selected file
  selected: string;

  onAdd(name: string): void;
  onRemove(name: string): void;
  onSelected(name: string): void;
  onRename(oldName: string, newName: string): void;
};

export function Files({ files, onAdd, onRemove, onRename }: Props) {
  return (
    <SideBar position="left">
      {files.map((file) => (
        <li key={file}>{file}</li>
      ))}
    </SideBar>
  );
}

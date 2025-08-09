import { Icons, Theme } from "shared";
import classNames from "classnames";
import { useState } from "react";
import { FileId } from "../Playground";
import { type FileHandle } from "ty_wasm";

export interface Props {
  // The file names
  files: ReadonlyArray<{ id: FileId; name: string }>;
  theme: Theme;
  selected: FileId;

  onAdd(name: string): void;

  onRemove(id: FileId): void;

  onSelect(id: FileId): void;

  onRename(id: FileId, newName: string): void;
}

export function Files({
  files,
  selected,
  theme,
  onAdd,
  onRemove,
  onRename,
  onSelect,
}: Props) {
  const handleAdd = () => {
    let index: number | null = null;
    let fileName = "module.py";

    while (files.some(({ name }) => name === fileName)) {
      index = (index ?? 0) + 1;
      fileName = `module${index}.py`;
    }

    onAdd(fileName);
  };

  const lastFile = files.length === 1;

  return (
    <ul
      className={classNames(
        "flex flex-wrap border-b border-gray-200",
        theme === "dark" ? "text-white border-rock" : null,
      )}
    >
      {files.map(({ id, name }) => (
        <ListItem key={id} selected={selected === id} theme={theme}>
          <FileEntry
            selected={selected === id}
            name={name}
            onClicked={() => onSelect(id)}
            onRenamed={(newName) => {
              if (!files.some(({ name }) => name === newName)) {
                onRename(id, newName);
              }
            }}
          />

          <button
            disabled={lastFile}
            onClick={lastFile ? undefined : () => onRemove(id)}
            className={"inline-block disabled:opacity-50 cursor-pointer"}
            title="Close file"
          >
            <span className="sr-only">Close</span>
            <Icons.Close />
          </button>
        </ListItem>
      ))}
      <ListItem selected={false} theme={theme}>
        <button
          onClick={handleAdd}
          title="Add file"
          className="inline-block cursor-pointer"
        >
          <span className="sr-only">Add file</span>
          <Icons.Add />
        </button>
      </ListItem>
    </ul>
  );
}

interface ListItemProps {
  selected: boolean;
  children: React.ReactNode;
  theme: Theme;
}

function ListItem({ children, selected, theme }: ListItemProps) {
  const activeBorderColor =
    theme === "light" ? "border-galaxy" : "border-radiate";

  return (
    <li
      aria-selected={selected}
      className={classNames(
        "flex",
        "px-4",
        "gap-2",
        "text-sm",
        "items-center",
        selected
          ? ["active", "border-b-2", "pb-0", activeBorderColor]
          : ["pb-0.5"],
      )}
    >
      {children}
    </li>
  );
}

interface FileEntryProps {
  selected: boolean;
  name: string;

  onClicked(): void;

  onRenamed(name: string): void;
}

function FileEntry({ name, onClicked, onRenamed, selected }: FileEntryProps) {
  const [newName, setNewName] = useState<string | null>(null);

  if (!selected && newName != null) {
    setNewName(null);
  }

  const handleRenamed = (newName: string) => {
    setNewName(null);
    if (name !== newName) {
      onRenamed(newName);
    }
  };

  const extension = name.split(".").pop()?.toLowerCase();

  const icon =
    extension === "py" || extension === "pyi" ? (
      <Icons.Python width={12} height={12} />
    ) : extension === "json" ? (
      <Icons.Json width={12} height={12} />
    ) : extension === "ipynb" ? (
      <Icons.Jupyter width={12} height={12} />
    ) : (
      <Icons.File width={12} height={12} />
    );

  return (
    <button
      onClick={() => {
        if (selected) {
          setNewName(name);
        } else {
          onClicked();
        }
      }}
      className="flex gap-2 items-center py-4 cursor-pointer"
    >
      <span className="inline-block flex-none" aria-hidden>
        {icon}
      </span>
      {newName == null ? (
        <span className="inline-block">{name}</span>
      ) : (
        <input
          className="inline-block"
          autoFocus={true}
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onBlur={() => handleRenamed(newName)}
          onKeyDown={(event) => {
            if (event.metaKey || event.altKey || event.shiftKey) {
              return;
            }

            switch (event.key) {
              case "Enter":
                event.currentTarget.blur();
                event.preventDefault();
                return;
              case "Escape":
                setNewName(null);
                return;
              case "\\":
                event.preventDefault();
            }
          }}
        />
      )}
    </button>
  );
}

export function isPythonFile(handle: FileHandle): boolean {
  const extension = handle.path().toLowerCase().split(".").pop() ?? "";
  return ["py", "pyi", "pyw"].includes(extension);
}

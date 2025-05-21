import {
  ActionDispatch,
  Suspense,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { ErrorMessage, Header, setupMonaco, useTheme } from "shared";
import { FileHandle, PositionEncoding, Workspace } from "ty_wasm";
import { persist, persistLocal, restore } from "./Editor/persist";
import { loader } from "@monaco-editor/react";
import tySchema from "../../../ty.schema.json";
import Chrome, { formatError } from "./Editor/Chrome";

export const SETTINGS_FILE_NAME = "ty.json";

export default function Playground() {
  const [theme, setTheme] = useTheme();
  const [version, setVersion] = useState<string>("0.0.0");
  const [error, setError] = useState<string | null>(null);
  const workspacePromiseRef = useRef<Promise<Workspace> | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);

  let workspacePromise = workspacePromiseRef.current;
  if (workspacePromise == null) {
    workspacePromiseRef.current = workspacePromise = startPlayground().then(
      (fetched) => {
        setVersion(fetched.version);
        const workspace = new Workspace("/", PositionEncoding.Utf16, {});
        restoreWorkspace(workspace, fetched.workspace, dispatchFiles, setError);
        setWorkspace(workspace);
        return workspace;
      },
    );
  }

  const [files, dispatchFiles] = useReducer(filesReducer, INIT_FILES_STATE);

  const fileName = useMemo(() => {
    return (
      files.index.find((file) => file.id === files.selected)?.name ?? "lib.py"
    );
  }, [files.index, files.selected]);

  usePersistLocally(files);

  const handleShare = useCallback(() => {
    const serialized = serializeFiles(files);

    if (serialized != null) {
      persist(serialized).catch((error) => {
        // eslint-disable-next-line no-console
        console.error("Failed to share playground", error);
      });
    }
  }, [files]);

  const handleFileAdded = (workspace: Workspace, name: string) => {
    let handle = null;

    if (name === SETTINGS_FILE_NAME) {
      updateOptions(workspace, "{}", setError);
    } else {
      handle = workspace.openFile(name, "");
    }

    dispatchFiles({ type: "add", name, handle, content: "" });
  };

  const handleFileChanged = (workspace: Workspace, content: string) => {
    if (files.selected == null) {
      return;
    }

    dispatchFiles({
      type: "change",
      id: files.selected,
      content,
    });

    const handle = files.handles[files.selected];

    if (handle != null) {
      updateFile(workspace, handle, content, setError);
    } else if (fileName === SETTINGS_FILE_NAME) {
      updateOptions(workspace, content, setError);
    }
  };

  const handleFileRenamed = (
    workspace: Workspace,
    file: FileId,
    newName: string,
  ) => {
    const handle = files.handles[file];
    let newHandle: FileHandle | null = null;
    if (handle == null) {
      updateOptions(workspace, null, setError);
    } else {
      workspace.closeFile(handle);
    }

    if (newName === SETTINGS_FILE_NAME) {
      updateOptions(workspace, files.contents[file], setError);
    } else {
      newHandle = workspace.openFile(newName, files.contents[file]);
    }

    dispatchFiles({ type: "rename", id: file, to: newName, newHandle });
  };

  const handleFileRemoved = (workspace: Workspace, file: FileId) => {
    const handle = files.handles[file];
    if (handle == null) {
      updateOptions(workspace, null, setError);
    } else {
      workspace.closeFile(handle);
    }

    dispatchFiles({ type: "remove", id: file });
  };

  const handleFileSelected = useCallback((file: FileId) => {
    dispatchFiles({ type: "selectFile", id: file });
  }, []);

  const handleReset = useCallback(() => {
    if (workspace == null) {
      return;
    }

    // Close all open files
    for (const file of files.index) {
      const handle = files.handles[file.id];

      if (handle != null) {
        try {
          workspace.closeFile(handle);
        } catch (e) {
          setError(formatError(e));
        }
      }
    }

    dispatchFiles({ type: "reset" });

    restoreWorkspace(workspace, DEFAULT_WORKSPACE, dispatchFiles, setError);
  }, [files.handles, files.index, workspace]);

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        edit={files.revision}
        theme={theme}
        tool="ty"
        version={version}
        onChangeTheme={setTheme}
        onShare={handleShare}
        onReset={workspace == null ? undefined : handleReset}
      />

      <Suspense fallback={<Loading />}>
        <Chrome
          files={files}
          workspacePromise={workspacePromise}
          theme={theme}
          selectedFileName={fileName}
          onAddFile={handleFileAdded}
          onRenameFile={handleFileRenamed}
          onRemoveFile={handleFileRemoved}
          onSelectFile={handleFileSelected}
          onChangeFile={handleFileChanged}
        />
      </Suspense>
      {error ? (
        <div
          style={{
            position: "fixed",
            left: "10%",
            right: "10%",
            bottom: "10%",
          }}
        >
          <ErrorMessage>{error}</ErrorMessage>
        </div>
      ) : null}
    </main>
  );
}

export const DEFAULT_SETTINGS = JSON.stringify(
  {
    environment: {
      "python-version": "3.13",
    },
    rules: {
      "division-by-zero": "error",
    },
  },
  null,
  4,
);

const DEFAULT_PROGRAM = `from typing import Literal

type Style = Literal["italic", "bold", "underline"]

# Add parameter annotations \`line: str, word: str, style: Style\` and a return
# type annotation \`-> str\` to see if you can find the mistakes in this program.

def with_style(line, word, style):
    if style == "italic":
        return line.replace(word, f"*{word}*")
    elif style == "bold":
        return line.replace(word, f"__{word}__")

    position = line.find(word)
    output = line + "\\n"
    output += " " * position
    output += "-" * len(word)


print(with_style("ty is a fast type checker for Python.", "fast", "underlined"))
`;

const DEFAULT_WORKSPACE = {
  files: {
    "main.py": DEFAULT_PROGRAM,
    "ty.json": DEFAULT_SETTINGS,
  },
  current: "main.py",
};

/**
 * Persists the files to local storage. This is done deferred to avoid too frequent writes.
 */
function usePersistLocally(files: FilesState): void {
  const deferredFiles = useDeferredValue(files);

  useEffect(() => {
    const serialized = serializeFiles(deferredFiles);
    if (serialized != null) {
      persistLocal(serialized);
    }
  }, [deferredFiles]);
}

export type FileId = number;

export type ReadonlyFiles = Readonly<FilesState>;

interface FilesState {
  /**
   * The currently selected file that is shown in the editor.
   */
  selected: FileId | null;

  /**
   * The files in display order (ordering is sensitive)
   */
  index: ReadonlyArray<{ id: FileId; name: string }>;

  /**
   * The database file handles by file id.
   *
   * Files without a file handle are well-known files that are only handled by the
   * playground (e.g. ty.json)
   */
  handles: Readonly<{ [id: FileId]: FileHandle | null }>;

  /**
   * The content per file indexed by file id.
   */
  contents: Readonly<{ [id: FileId]: string }>;

  /**
   * The revision. Gets incremented every time files changes.
   */
  revision: number;

  /**
   * Revision identifying this playground. Gets incremented every time the
   * playground is reset.
   */
  playgroundRevision: number;

  nextId: FileId;
}

export type FileAction =
  | {
      type: "add";
      handle: FileHandle | null;
      /// The file name
      name: string;
      content: string;
    }
  | {
      type: "change";
      id: FileId;
      content: string;
    }
  | { type: "rename"; id: FileId; to: string; newHandle: FileHandle | null }
  | {
      type: "remove";
      id: FileId;
    }
  | { type: "selectFile"; id: FileId }
  | { type: "selectFileByName"; name: string }
  | { type: "reset" };

const INIT_FILES_STATE: ReadonlyFiles = {
  index: [],
  contents: Object.create(null),
  handles: Object.create(null),
  nextId: 0,
  revision: 0,
  selected: null,
  playgroundRevision: 0,
};

function filesReducer(
  state: Readonly<FilesState>,
  action: FileAction,
): FilesState {
  switch (action.type) {
    case "add": {
      const { handle, name, content } = action;
      const id = state.nextId;
      return {
        ...state,
        selected: id,
        index: [...state.index, { id, name }],
        handles: { ...state.handles, [id]: handle },
        contents: { ...state.contents, [id]: content },
        nextId: state.nextId + 1,
        revision: state.revision + 1,
      };
    }

    case "change": {
      const { id, content } = action;
      return {
        ...state,
        contents: { ...state.contents, [id]: content },
        revision: state.revision + 1,
      };
    }

    case "remove": {
      const { id } = action;

      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { [id]: _content, ...contents } = state.contents;
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { [id]: _handle, ...handles } = state.handles;

      let selected = state.selected;

      if (state.selected === id) {
        const index = state.index.findIndex((file) => file.id === id);

        selected =
          index > 0 ? state.index[index - 1].id : state.index[index + 1].id;
      }

      return {
        ...state,
        selected,
        index: state.index.filter((file) => file.id !== id),
        contents,
        handles,
        revision: state.revision + 1,
      };
    }
    case "rename": {
      const { id, to, newHandle } = action;

      const index = state.index.findIndex((file) => file.id === id);
      const newIndex = [...state.index];
      newIndex.splice(index, 1, { id, name: to });

      return {
        ...state,
        index: newIndex,
        handles: { ...state.handles, [id]: newHandle },
      };
    }

    case "selectFile": {
      const { id } = action;

      return {
        ...state,
        selected: id,
      };
    }

    case "selectFileByName": {
      const { name } = action;

      const selected =
        state.index.find((file) => file.name === name)?.id ?? null;

      return {
        ...state,
        selected,
      };
    }

    case "reset": {
      return {
        ...INIT_FILES_STATE,
        playgroundRevision: state.playgroundRevision + 1,
        revision: state.revision + 1,
      };
    }
  }
}

function serializeFiles(files: FilesState): {
  files: { [name: string]: string };
  current: string;
} | null {
  const serializedFiles = Object.create(null);
  let selected = null;

  for (const { id, name } of files.index) {
    serializedFiles[name] = files.contents[id];

    if (files.selected === id) {
      selected = name;
    }
  }

  if (selected == null) {
    return null;
  }

  return { files: serializedFiles, current: selected };
}

export interface InitializedPlayground {
  version: string;
  workspace: { files: { [name: string]: string }; current: string };
}

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<InitializedPlayground> {
  const ty = await import("../ty_wasm");
  await ty.default();
  const monaco = await loader.init();

  setupMonaco(monaco, {
    uri: "https://raw.githubusercontent.com/astral-sh/ruff/main/ty.schema.json",
    fileMatch: ["ty.json"],
    schema: tySchema,
  });

  const restored = await restore();

  const workspace = restored ?? DEFAULT_WORKSPACE;

  return {
    version: "0.0.0",
    workspace,
  };
}

function updateOptions(
  workspace: Workspace | null,
  content: string | null,
  setError: (error: string | null) => void,
) {
  content = content ?? DEFAULT_SETTINGS;

  try {
    const settings = JSON.parse(content);
    workspace?.updateOptions(settings);
    setError(null);
  } catch (error) {
    setError(`Failed to update 'ty.json' options: ${formatError(error)}`);
  }
}

function updateFile(
  workspace: Workspace,
  handle: FileHandle,
  content: string,
  setError: (error: string | null) => void,
) {
  try {
    workspace.updateFile(handle, content);
    setError(null);
  } catch (error) {
    setError(`Failed to update file: ${formatError(error)}`);
  }
}

function Loading() {
  return (
    <div className="align-middle text-current text-center my-2 dark:text-white">
      Loading...
    </div>
  );
}

function restoreWorkspace(
  workspace: Workspace,
  state: {
    files: { [name: string]: string };
    current: string;
  },
  dispatchFiles: ActionDispatch<[FileAction]>,
  setError: (error: string | null) => void,
) {
  let hasSettings = false;

  // eslint-disable-next-line prefer-const
  for (let [name, content] of Object.entries(state.files)) {
    let handle = null;

    if (
      name === "knot.json" &&
      !Object.keys(state.files).includes(SETTINGS_FILE_NAME)
    ) {
      name = SETTINGS_FILE_NAME;
    }

    if (name === SETTINGS_FILE_NAME) {
      updateOptions(workspace, content, setError);
      hasSettings = true;
    } else {
      handle = workspace.openFile(name, content);
    }

    dispatchFiles({ type: "add", handle, content, name });
  }

  if (!hasSettings) {
    updateOptions(workspace, null, setError);
  }

  const selected =
    state.current === "knot.json" ? SETTINGS_FILE_NAME : state.current;

  dispatchFiles({
    type: "selectFileByName",
    name: selected,
  });
}

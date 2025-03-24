import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  Header,
  useTheme,
  setupMonaco,
  ErrorMessage,
  HorizontalResizeHandle,
  VerticalResizeHandle,
} from "shared";
import knotSchema from "../../../../knot.schema.json";
import initRedKnot, { Diagnostic, FileHandle, Workspace } from "red_knot_wasm";
import { loader } from "@monaco-editor/react";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Files } from "./Files";
import { persist, persistLocal, restore } from "./persist";
import SecondarySideBar from "./SecondarySideBar";
import Editor from "./Editor";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import Diagnostics from "./Diagnostics";
import { editor } from "monaco-editor";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

const SETTINGS_FILE = "knot.json";

interface CheckResult {
  diagnostics: Diagnostic[];
  error: string | null;
  secondary: SecondaryPanelResult;
}

export default function Chrome() {
  const initPromise = useRef<null | Promise<void>>(null);
  const [workspace, setWorkspace] = useState<null | Workspace>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [files, dispatchFiles] = useReducer(filesReducer, {
    index: [],
    contents: Object.create(null),
    handles: Object.create(null),
    nextId: 0,
    revision: 0,
    selected: null,
  });
  const [secondaryTool, setSecondaryTool] = useState<SecondaryTool | null>(
    null,
  );

  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const [version, setVersion] = useState("");
  const [theme, setTheme] = useTheme();

  const fileName = useMemo(() => {
    return files.index.find((file) => file.id === files.selected)?.name ?? null;
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

  if (initPromise.current == null) {
    initPromise.current = startPlayground()
      .then(({ version, workspace: fetchedWorkspace }) => {
        const workspace = new Workspace("/", {});

        setVersion(version);
        setWorkspace(workspace);

        let hasSettings = false;

        for (const [name, content] of Object.entries(fetchedWorkspace.files)) {
          let handle = null;
          if (name === SETTINGS_FILE) {
            updateOptions(workspace, content, setUpdateError);
            hasSettings = true;
          } else {
            handle = workspace.openFile(name, content);
          }

          dispatchFiles({ type: "add", handle, name, content });
        }

        if (!hasSettings) {
          updateOptions(workspace, null, setUpdateError);
        }

        dispatchFiles({
          type: "selectFileByName",
          name: fetchedWorkspace.current,
        });
      })
      .catch((error) => {
        // eslint-disable-next-line no-console
        console.error("Failed to initialize playground.", error);
      });
  }

  const handleSourceChanged = useCallback(
    (source: string) => {
      if (files.selected == null) {
        return;
      }

      dispatchFiles({
        type: "change",
        id: files.selected,
        content: source,
      });

      const handle = files.handles[files.selected];

      if (handle != null) {
        updateFile(workspace, handle, source, setUpdateError);
      } else if (fileName === SETTINGS_FILE) {
        updateOptions(workspace, source, setUpdateError);
      }
    },
    [files.selected, files.handles, fileName, workspace],
  );

  const handleFileClicked = useCallback((file: FileId) => {
    dispatchFiles({ type: "selectFile", id: file });
  }, []);

  const handleFileAdded = useCallback(
    (name: string) => {
      if (workspace == null) {
        return;
      }

      let handle = null;

      if (name === SETTINGS_FILE) {
        updateOptions(workspace, "{}", setUpdateError);
      } else {
        handle = workspace.openFile(name, "");
      }

      dispatchFiles({ type: "add", name, handle, content: "" });
    },
    [workspace],
  );

  const handleFileRemoved = useCallback(
    (file: FileId) => {
      if (workspace != null) {
        const handle = files.handles[file];
        if (handle == null) {
          updateOptions(workspace, null, setUpdateError);
        } else {
          workspace.closeFile(handle);
        }
      }

      dispatchFiles({ type: "remove", id: file });
    },
    [workspace, files.handles],
  );

  const handleFileRenamed = useCallback(
    (file: FileId, newName: string) => {
      if (workspace == null) {
        return;
      }

      const handle = files.handles[file];
      let newHandle: FileHandle | null = null;
      if (handle == null) {
        updateOptions(workspace, null, setUpdateError);
      } else {
        workspace.closeFile(handle);
      }

      if (newName === SETTINGS_FILE) {
        updateOptions(workspace, files.contents[file], setUpdateError);
      } else {
        newHandle = workspace.openFile(newName, files.contents[file]);
      }

      editorRef.current?.focus();

      dispatchFiles({ type: "rename", id: file, to: newName, newHandle });
    },
    [workspace, files.handles, files.contents],
  );

  const handleSecondaryToolSelected = useCallback(
    (tool: SecondaryTool | null) => {
      setSecondaryTool((secondaryTool) => {
        if (tool === secondaryTool) {
          return null;
        }

        return tool;
      });
    },
    [],
  );

  const handleEditorMount = useCallback((editor: IStandaloneCodeEditor) => {
    editorRef.current = editor;
  }, []);

  const handleGoTo = useCallback((line: number, column: number) => {
    const editor = editorRef.current;

    if (editor == null) {
      return;
    }

    const range = {
      startLineNumber: line,
      startColumn: column,
      endLineNumber: line,
      endColumn: column,
    };
    editor.revealRange(range);
    editor.setSelection(range);
  }, []);

  const checkResult = useCheckResult(files, workspace, secondaryTool);
  const error = updateError ?? checkResult.error;

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        edit={files.revision}
        theme={theme}
        logo="astral"
        version={version}
        onChangeTheme={setTheme}
        onShare={handleShare}
      />

      {workspace != null && files.selected != null ? (
        <>
          <Files
            files={files.index}
            theme={theme}
            selected={files.selected}
            onAdd={handleFileAdded}
            onRename={handleFileRenamed}
            onSelected={handleFileClicked}
            onRemove={handleFileRemoved}
          />
          <PanelGroup direction="horizontal" autoSaveId="main">
            <Panel
              id="main"
              order={0}
              className="flex flex-col gap-2 my-4"
              minSize={10}
            >
              <PanelGroup id="vertical" direction="vertical">
                <Panel minSize={10} className="my-2" order={0}>
                  <Editor
                    key={fileName}
                    theme={theme}
                    visible={true}
                    fileName={fileName ?? "lib.py"}
                    source={files.contents[files.selected]}
                    diagnostics={checkResult.diagnostics}
                    workspace={workspace}
                    onMount={handleEditorMount}
                    onChange={handleSourceChanged}
                  />
                  <VerticalResizeHandle />
                </Panel>
                <Panel
                  id="diagnostics"
                  minSize={3}
                  order={1}
                  className="my-2 flex grow"
                >
                  <Diagnostics
                    diagnostics={checkResult.diagnostics}
                    workspace={workspace}
                    onGoTo={handleGoTo}
                    theme={theme}
                  />
                </Panel>
              </PanelGroup>
            </Panel>
            {secondaryTool != null && (
              <>
                <HorizontalResizeHandle />
                <Panel
                  id="secondary-panel"
                  order={1}
                  className={"my-2"}
                  minSize={10}
                >
                  <SecondaryPanel
                    theme={theme}
                    tool={secondaryTool}
                    result={checkResult.secondary}
                  />
                </Panel>
              </>
            )}
            <SecondarySideBar
              selected={secondaryTool}
              onSelected={handleSecondaryToolSelected}
            />
          </PanelGroup>
        </>
      ) : null}

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

const DEFAULT_SETTINGS = JSON.stringify(
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

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<{
  version: string;
  workspace: { files: { [name: string]: string }; current: string };
}> {
  await initRedKnot();
  const monaco = await loader.init();

  setupMonaco(monaco, {
    uri: "https://raw.githubusercontent.com/astral-sh/ruff/main/knot.schema.json",
    fileMatch: ["knot.json"],
    schema: knotSchema,
  });

  const restored = await restore();

  const workspace = restored ?? {
    files: {
      "main.py": "import os",
      "knot.json": DEFAULT_SETTINGS,
    },
    current: "main.py",
  };

  return {
    version: "0.0.0",
    workspace,
  };
}

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

function useCheckResult(
  files: FilesState,
  workspace: Workspace | null,
  secondaryTool: SecondaryTool | null,
): CheckResult {
  const deferredContent = useDeferredValue(
    files.selected == null ? null : files.contents[files.selected],
  );

  return useMemo(() => {
    if (
      workspace == null ||
      files.selected == null ||
      deferredContent == null
    ) {
      return {
        diagnostics: [],
        error: null,
        secondary: null,
      };
    }

    const currentHandle = files.handles[files.selected];

    if (currentHandle == null) {
      return {
        diagnostics: [],
        error: null,
        secondary: null,
      };
    }

    try {
      const diagnostics = workspace.checkFile(currentHandle);

      let secondary: SecondaryPanelResult = null;

      try {
        switch (secondaryTool) {
          case "AST":
            secondary = {
              status: "ok",
              content: workspace.parsed(currentHandle),
            };
            break;

          case "Tokens":
            secondary = {
              status: "ok",
              content: workspace.tokens(currentHandle),
            };
            break;
        }
      } catch (error: unknown) {
        secondary = {
          status: "error",
          error: error instanceof Error ? error.message : error + "",
        };
      }

      return {
        diagnostics,
        error: null,
        secondary,
      };
    } catch (e) {
      // eslint-disable-next-line no-console
      console.error(e);

      return {
        diagnostics: [],
        error: formatError(e),
        secondary: null,
      };
    }
  }, [
    deferredContent,
    workspace,
    files.selected,
    files.handles,
    secondaryTool,
  ]);
}

export type FileId = number;

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
   * playground (e.g. knot.json)
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
  nextId: FileId;
}

type FileAction =
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
  | { type: "selectFileByName"; name: string };

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

function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}

function updateOptions(
  workspace: Workspace | null,
  content: string | null,
  setError: (error: string | null) => void,
) {
  if (workspace == null) {
    return;
  }

  content = content ?? DEFAULT_SETTINGS;

  try {
    const settings = JSON.parse(content);
    workspace?.updateOptions(settings);
    setError(null);
  } catch (error) {
    setError(`Failed to update 'knot.json' options: ${formatError(error)}`);
  }
}

function updateFile(
  workspace: Workspace | null,
  handle: FileHandle,
  content: string,
  setError: (error: string | null) => void,
) {
  if (workspace == null) {
    return;
  }

  try {
    workspace.updateFile(handle, content);
    setError(null);
  } catch (error) {
    setError(`Failed to update file: ${formatError(error)}`);
  }
}

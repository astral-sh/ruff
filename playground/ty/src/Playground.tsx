import {
  ActionDispatch,
  RefObject,
  Suspense,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  ErrorMessage,
  Header,
  setupMonaco,
  useTheme,
  downloadZip,
} from "shared";
import { FileHandle, PositionEncoding, Workspace } from "ty_wasm";
import {
  copyAsMarkdown,
  copyAsMarkdownLink,
  persist,
  persistLocal,
  restore,
} from "./Editor/persist";
import { loader } from "@monaco-editor/react";
import tySchema from "../../../ty.schema.json";
import Chrome, { formatError } from "./Editor/Chrome";
import { isPythonFile } from "./Editor/Files";
import type { Monaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";

export const SETTINGS_FILE_NAME = "ty.json";

export default function Playground() {
  const [theme, setTheme] = useTheme();
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [files, dispatchFiles] = useReducer(filesReducer, INIT_FILES_STATE);
  const [documentRevision, bumpDocumentRevision] = useReducer(
    (revision) => revision + 1,
    0,
  );
  const documentStoreRef = useRef<MonacoDocumentStore | null>(null);

  const workspacePromiseRef = useRef<Promise<Workspace> | null>(null);
  if (workspacePromiseRef.current == null) {
    workspacePromiseRef.current = startPlayground().then((fetched) => {
      setVersion(fetched.version);
      const workspace = new Workspace("/", PositionEncoding.Utf16, {});
      const documentStore = new MonacoDocumentStore(
        fetched.monaco,
        workspace,
        setError,
        bumpDocumentRevision,
      );
      documentStoreRef.current = documentStore;
      restoreWorkspace(
        workspace,
        documentStore,
        fetched.workspace,
        dispatchFiles,
        setError,
      );
      setWorkspace(workspace);
      return workspace;
    });
  }
  // This is safe as this is only called once on startup.
  // We need useRef to avoid duplicate initialization when
  // running locally due to react rendering
  // everything twice in strict mode in debug builds.
  const workspacePromise = workspacePromiseRef.current;

  const fileName = useMemo(() => {
    return (
      files.index.find((file) => file.id === files.selected)?.name ?? "lib.py"
    );
  }, [files.index, files.selected]);

  usePersistLocally(files, documentStoreRef, documentRevision);

  const handleShare = useCallback(async () => {
    const serializedFiles = serializeFiles(files, documentStoreRef.current);

    if (serializedFiles != null) {
      await persist(serializedFiles);
    }
  }, [files]);

  const handleCopyMarkdown = useCallback(async () => {
    const serializedFiles = serializeFiles(files, documentStoreRef.current);

    if (serializedFiles != null) {
      await copyAsMarkdown(serializedFiles);
    }
  }, [files]);

  const handleCopyMarkdownLink = useCallback(async () => {
    const serializedFiles = serializeFiles(files, documentStoreRef.current);

    if (serializedFiles != null) {
      await copyAsMarkdownLink(serializedFiles);
    }
  }, [files]);

  const handleDownload = useCallback(async () => {
    const serializedFiles = serializeFiles(files, documentStoreRef.current);

    if (serializedFiles != null) {
      const downloadFiles = { ...serializedFiles.files };

      if (SETTINGS_FILE_NAME in downloadFiles) {
        try {
          const toml = await import("smol-toml");
          const tomlContent = toml.stringify(
            JSON.parse(downloadFiles[SETTINGS_FILE_NAME]),
          );
          delete downloadFiles[SETTINGS_FILE_NAME];
          downloadFiles["ty.toml"] = tomlContent;
        } catch {
          // Keep the original JSON file if conversion fails.
        }
      }

      await downloadZip(downloadFiles, "ty-playground");
    }
  }, [files]);

  const handleRun = useCallback(async () => {
    const serializedFiles = serializeFiles(files, documentStoreRef.current);
    return serializedFiles == null ? "" : runPython(serializedFiles);
  }, [files]);

  const handleFileAdded = useCallback((workspace: Workspace, name: string) => {
    const documentStore = documentStoreRef.current;
    if (documentStore == null) {
      return;
    }

    let handle = null;

    if (name === SETTINGS_FILE_NAME) {
      updateOptions(workspace, "{}", setError);
    } else {
      handle = workspace.openFile(name, "");
    }

    documentStore.openDocument(name, "", handle);
    dispatchFiles({ type: "add", name, handle });
  }, []);

  const handleFileRenamed = useCallback(
    (workspace: Workspace, file: FileId, newName: string) => {
      if (newName.startsWith("/")) {
        setError("File names cannot start with '/'.");
        return;
      }
      if (newName.startsWith("vendored:")) {
        setError("File names cannot start with 'vendored:'.");
        return;
      }

      const documentStore = documentStoreRef.current;
      if (documentStore == null) {
        return;
      }

      const oldName = files.index.find(({ id }) => id === file)?.name;
      if (oldName == null) {
        return;
      }

      const content = documentStore.text(oldName) ?? "";
      const handle = files.handles[file];
      let newHandle: FileHandle | null = null;
      if (handle == null) {
        updateOptions(workspace, null, setError);
      } else {
        workspace.closeFile(handle);
      }

      if (newName === SETTINGS_FILE_NAME) {
        updateOptions(workspace, content, setError);
      } else {
        newHandle = workspace.openFile(newName, content);
      }

      documentStore.renameDocument(oldName, newName, newHandle);
      dispatchFiles({ type: "rename", id: file, to: newName, newHandle });
    },
    [files.handles, files.index],
  );

  const handleFileRemoved = useCallback(
    (workspace: Workspace, file: FileId) => {
      const documentStore = documentStoreRef.current;
      const name = files.index.find(({ id }) => id === file)?.name;
      const handle = files.handles[file];
      if (handle == null) {
        updateOptions(workspace, null, setError);
      } else {
        workspace.closeFile(handle);
      }

      if (name != null) {
        documentStore?.closeDocument(name);
      }
      dispatchFiles({ type: "remove", id: file });
    },
    [files.handles, files.index],
  );

  const handleFileSelected = useCallback((file: FileId) => {
    dispatchFiles({ type: "selectFile", id: file });
  }, []);

  const handleVendoredFileSelected = useCallback((handle: FileHandle) => {
    dispatchFiles({ type: "selectVendoredFile", handle });
  }, []);

  const handleVendoredFileCleared = useCallback(() => {
    dispatchFiles({ type: "clearVendoredFile" });
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

    documentStoreRef.current?.closeDocuments(
      files.index.map((file) => file.name),
    );
    dispatchFiles({ type: "reset" });

    const documentStore = documentStoreRef.current;
    if (documentStore != null) {
      restoreWorkspace(
        workspace,
        documentStore,
        DEFAULT_WORKSPACE,
        dispatchFiles,
        setError,
      );
    }
  }, [files.handles, files.index, workspace]);

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        theme={theme}
        tool="ty"
        version={version}
        onChangeTheme={setTheme}
        edit={files.revision + documentRevision}
        onShare={handleShare}
        onCopyMarkdownLink={handleCopyMarkdownLink}
        onCopyMarkdown={handleCopyMarkdown}
        onDownload={handleDownload}
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
          documentRevision={documentRevision}
          onRun={handleRun}
          onSelectVendoredFile={handleVendoredFileSelected}
          onClearVendoredFile={handleVendoredFileCleared}
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
      "python-version": "3.14",
    },
    rules: {
      "undefined-reveal": "ignore",
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
function usePersistLocally(
  files: FilesState,
  documentStoreRef: RefObject<MonacoDocumentStore | null>,
  documentRevision: number,
): void {
  const deferredFiles = useDeferredValue(files);
  const deferredDocumentRevision = useDeferredValue(documentRevision);

  useEffect(() => {
    const serialized = serializeFiles(deferredFiles, documentStoreRef.current);
    if (serialized != null) {
      persistLocal(serialized);
    }
  }, [deferredFiles, deferredDocumentRevision, documentStoreRef]);
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
   * The revision. Gets incremented every time files changes.
   */
  revision: number;

  /**
   * Revision identifying this playground. Gets incremented every time the
   * playground is reset.
   */
  playgroundRevision: number;

  nextId: FileId;

  /**
   * The currently viewed vendored/builtin file, if any.
   */
  currentVendoredFile: FileHandle | null;
}

export type FileAction =
  | {
      type: "add";
      handle: FileHandle | null;
      /// The file name
      name: string;
    }
  | { type: "rename"; id: FileId; to: string; newHandle: FileHandle | null }
  | {
      type: "remove";
      id: FileId;
    }
  | { type: "selectFile"; id: FileId }
  | { type: "selectFileByName"; name: string }
  | { type: "reset" }
  | {
      type: "selectVendoredFile";
      handle: FileHandle;
    }
  | { type: "clearVendoredFile" };

const INIT_FILES_STATE: ReadonlyFiles = {
  index: [],
  handles: Object.create(null),
  nextId: 0,
  revision: 0,
  selected: null,
  playgroundRevision: 0,
  currentVendoredFile: null,
};

function filesReducer(
  state: Readonly<FilesState>,
  action: FileAction,
): FilesState {
  switch (action.type) {
    case "add": {
      const { handle, name } = action;
      const id = state.nextId;
      return {
        ...state,
        selected: id,
        index: [...state.index, { id, name }],
        handles: { ...state.handles, [id]: handle },
        nextId: state.nextId + 1,
        revision: state.revision + 1,
        currentVendoredFile: null, // Clear vendored file when adding new file
      };
    }

    case "remove": {
      const { id } = action;

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
        handles,
        revision: state.revision + 1,
        currentVendoredFile: null, // Clear vendored file when removing file
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
        currentVendoredFile: null, // Clear vendored file when selecting regular file
      };
    }

    case "selectFileByName": {
      const { name } = action;

      const selected =
        state.index.find((file) => file.name === name)?.id ?? null;

      return {
        ...state,
        selected,
        currentVendoredFile: null, // Clear vendored file when selecting regular file
      };
    }

    case "reset": {
      return {
        ...INIT_FILES_STATE,
        playgroundRevision: state.playgroundRevision + 1,
        revision: state.revision + 1,
      };
    }

    case "selectVendoredFile": {
      const { handle } = action;

      return {
        ...state,
        currentVendoredFile: handle,
      };
    }

    case "clearVendoredFile": {
      return {
        ...state,
        currentVendoredFile: null,
      };
    }
  }
}

export interface SerializedFiles {
  files: { [name: string]: string };
  current: string;
}

function serializeFiles(
  files: FilesState,
  documentStore: MonacoDocumentStore | null,
): SerializedFiles | null {
  if (documentStore == null) {
    return null;
  }

  const serializedFiles = Object.create(null);
  let selected = null;

  for (const { id, name } of files.index) {
    const text = documentStore.text(name);
    if (text == null) {
      return null;
    }

    serializedFiles[name] = text;

    if (files.selected === id) {
      selected = name;
    }
  }

  if (selected == null) {
    return null;
  }

  return { files: serializedFiles, current: selected };
}

const SANDBOX_BASE_DIRECTORY = "/playground/";

async function runPython(workspace: SerializedFiles): Promise<string> {
  const { loadPyodide } = await import("pyodide");
  const pyodide = await loadPyodide({
    env: {
      HOME: SANDBOX_BASE_DIRECTORY,
    },
  });

  let combinedOutput = "";

  const outputHandler = (output: string) => {
    combinedOutput += output + "\n";
  };

  pyodide.setStdout({ batched: outputHandler });
  pyodide.setStderr({ batched: outputHandler });

  for (const [fileName, content] of Object.entries(workspace.files)) {
    const lastSeparator = fileName.lastIndexOf("/");

    if (lastSeparator !== -1) {
      const directory =
        SANDBOX_BASE_DIRECTORY + fileName.slice(0, lastSeparator);
      pyodide.FS.mkdirTree(directory);
    }

    pyodide.FS.writeFile(SANDBOX_BASE_DIRECTORY + fileName, content);
  }

  const dict = pyodide.globals.get("dict");
  const globals = dict();

  try {
    // Patch `reveal_type` to print runtime values
    pyodide.runPython(`
        import builtins

        def reveal_type(obj):
          import typing
          print(f"Runtime value is '{obj}'")
          return typing.reveal_type(obj)

        builtins.reveal_type = reveal_type`);

    pyodide.runPython(workspace.files[workspace.current] ?? "", {
      globals,
      locals: globals,
      filename: workspace.current,
    });

    return combinedOutput;
  } catch (error) {
    return `Failed to run Python script: ${error}`;
  } finally {
    globals.destroy();
    dict.destroy();
  }
}

export interface InitializedPlayground {
  version: string;
  monaco: Monaco;
  workspace: { files: { [name: string]: string }; current: string };
}

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<InitializedPlayground> {
  const ty = await import("ty_wasm");
  await ty.default();

  if (import.meta.env.DEV) {
    ty.initLogging(ty.LogLevel.Debug);
  } else {
    ty.initLogging(ty.LogLevel.Info);
  }

  const version = ty.version();
  const monaco = await loader.init();

  setupMonaco(monaco, {
    uri: "https://raw.githubusercontent.com/astral-sh/ruff/main/ty.schema.json",
    fileMatch: ["ty.json"],
    schema: tySchema,
  });

  const restored = await restore();

  const workspace = restored ?? DEFAULT_WORKSPACE;

  return {
    version,
    monaco,
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

class MonacoDocumentStore {
  constructor(
    private monaco: Monaco,
    private workspace: Workspace,
    private setError: (error: string | null) => void,
    private onChanged: () => void,
  ) {}

  openDocument(
    name: string,
    content: string,
    handle: FileHandle | null,
  ): editor.ITextModel {
    this.closeDocument(name);

    const model = this.monaco.editor.createModel(
      content,
      languageForFile(handle ?? name),
      this.uri(name),
    );
    this.registerWorkspaceSync(name, handle, model);

    return model;
  }

  renameDocument(
    oldName: string,
    newName: string,
    newHandle: FileHandle | null,
  ): editor.ITextModel {
    const content = this.text(oldName) ?? "";
    this.closeDocument(oldName);
    const model = this.openDocument(newName, content, newHandle);
    this.onChanged();
    return model;
  }

  closeDocument(name: string): void {
    this.model(name)?.dispose();
  }

  closeDocuments(names: Iterable<string>): void {
    for (const name of names) {
      this.closeDocument(name);
    }
  }

  text(name: string): string | null {
    return this.model(name)?.getValue() ?? null;
  }

  private registerWorkspaceSync(
    name: string,
    handle: FileHandle | null,
    model: editor.ITextModel,
  ): void {
    const syncDisposable = model.onDidChangeContent(() => {
      const content = model.getValue();

      if (handle != null) {
        updateFile(this.workspace, handle, content, this.setError);
      } else if (name === SETTINGS_FILE_NAME) {
        updateOptions(this.workspace, content, this.setError);
      }

      this.onChanged();
    });

    model.onWillDispose(() => syncDisposable.dispose());
  }

  private model(name: string): editor.ITextModel | null {
    return this.monaco.editor.getModel(this.uri(name));
  }

  private uri(name: string) {
    return this.monaco.Uri.parse(name);
  }
}

function languageForFile(file: FileHandle | string): string | undefined {
  if (typeof file === "string") {
    return file.endsWith(".py") || file.endsWith(".pyi") || file.endsWith(".pyw")
      ? "python"
      : undefined;
  }

  return isPythonFile(file) ? "python" : undefined;
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
  documentStore: MonacoDocumentStore,
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

    documentStore.openDocument(name, content, handle);
    dispatchFiles({ type: "add", handle, name });
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

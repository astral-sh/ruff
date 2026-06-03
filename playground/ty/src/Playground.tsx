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
import { runPython } from "./Editor/runPython";
import type { Monaco } from "@monaco-editor/react";
import type { editor, Uri } from "monaco-editor";

export const SETTINGS_FILE_NAME = "ty.json";

export default function Playground() {
  const [theme, setTheme] = useTheme();
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [files, dispatchFiles] = useReducer(filesReducer, INIT_FILES_STATE);
  const [session, setSession] = useState<PlaygroundSession | null>(null);

  const sessionPromiseRef = useRef<Promise<PlaygroundSession> | null>(null);
  if (sessionPromiseRef.current == null) {
    sessionPromiseRef.current = startPlayground().then((fetched) => {
      setVersion(fetched.version);
      const workspace = new Workspace("/", PositionEncoding.Utf16, {});
      const session = new PlaygroundSession(
        fetched.monaco,
        workspace,
        setError,
        () => dispatchFiles({ type: "documentChanged" }),
      );
      restoreWorkspace(session, fetched.workspace, dispatchFiles, setError);
      setSession(session);
      return session;
    });
  }
  // This is safe as this is only called once on startup.
  // We need useRef to avoid duplicate initialization when
  // running locally due to react rendering
  // eslint-disable-next-line
  const sessionPromise = sessionPromiseRef.current;

  const fileName = useMemo(() => {
    return files.selected == null
      ? "lib.py"
      : files.metadata[files.selected].name;
  }, [files.metadata, files.selected]);

  usePersistLocally(files, session);

  const handleShare = useCallback(async () => {
    const serialized = serializeFiles(files, session);

    if (serialized != null) {
      await persist(serialized);
    }
  }, [session, files]);

  const handleCopyMarkdown = useCallback(async () => {
    const serialized = serializeFiles(files, session);

    if (serialized != null) {
      await copyAsMarkdown(serialized);
    }
  }, [session, files]);

  const handleCopyMarkdownLink = useCallback(async () => {
    const serialized = serializeFiles(files, session);

    if (serialized != null) {
      await copyAsMarkdownLink(serialized);
    }
  }, [session, files]);

  const handleDownload = useCallback(async () => {
    const serialized = serializeFiles(files, session);

    if (serialized != null) {
      const downloadFiles = { ...serialized.files };

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
  }, [session, files]);

  const handleRun = useCallback(async () => {
    const serialized = serializeFiles(files, session);
    return serialized == null ? "" : runPython(serialized);
  }, [session, files]);

  const handleFileAdded = useCallback(
    (session: PlaygroundSession, name: string) => {
      const workspace = session.workspace;
      let handle = null;

      if (name === SETTINGS_FILE_NAME) {
        updateOptions(workspace, "{}", setError);
      } else {
        handle = workspace.openFile(name, "");
      }

      const model = session.openDocument(name, "", handle);
      dispatchFiles({
        type: "add",
        name,
        uri: model.uri,
        handle,
      });
    },
    [],
  );

  const handleFileRenamed = useCallback(
    (session: PlaygroundSession, file: FileId, newName: string) => {
      if (newName.startsWith("/")) {
        setError("File names cannot start with '/'.");
        return;
      }
      if (newName.startsWith("vendored:")) {
        setError("File names cannot start with 'vendored:'.");
        return;
      }

      const workspace = session.workspace;
      const oldFile = files.metadata[file];
      const oldName = oldFile.name;
      const content = session.text(oldName) ?? "";
      const handle = oldFile.handle;
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

      const model = session.renameDocument(oldName, newName, newHandle);
      dispatchFiles({
        type: "rename",
        id: file,
        to: newName,
        newUri: model.uri,
        newHandle,
      });
    },
    [files.metadata],
  );

  const handleFileRemoved = useCallback(
    (session: PlaygroundSession, file: FileId) => {
      const workspace = session.workspace;
      const removedFile = files.metadata[file];
      const { handle, name } = removedFile;
      if (handle == null) {
        updateOptions(workspace, null, setError);
      } else {
        workspace.closeFile(handle);
      }

      session.closeDocument(name);
      dispatchFiles({ type: "remove", id: file });
    },
    [files.metadata],
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
    if (session == null) {
      return;
    }

    const workspace = session.workspace;

    // Close all open files
    for (const file of Object.values(files.metadata)) {
      if (file.handle != null) {
        try {
          workspace.closeFile(file.handle);
        } catch (e) {
          setError(formatError(e));
        }
      }
    }

    session.closeDocuments(
      Object.values(files.metadata).map((file) => file.name),
    );
    dispatchFiles({ type: "reset" });

    restoreWorkspace(session, DEFAULT_WORKSPACE, dispatchFiles, setError);
  }, [session, files]);

  return (
    <main className="flex flex-col h-full bg-white dark:bg-ayu-background-dark">
      <Header
        theme={theme}
        tool="ty"
        version={version}
        onChangeTheme={setTheme}
        edit={files.revision}
        onShare={handleShare}
        onCopyMarkdownLink={handleCopyMarkdownLink}
        onCopyMarkdown={handleCopyMarkdown}
        onDownload={handleDownload}
        onReset={session == null ? undefined : handleReset}
      />

      <Suspense fallback={<Loading />}>
        <Chrome
          files={files}
          sessionPromise={sessionPromise}
          theme={theme}
          selectedFileName={fileName}
          onAddFile={handleFileAdded}
          onRenameFile={handleFileRenamed}
          onRemoveFile={handleFileRemoved}
          onSelectFile={handleFileSelected}
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
  session: PlaygroundSession | null,
): void {
  const deferredFiles = useDeferredValue(files);

  useEffect(() => {
    const serialized = serializeFiles(deferredFiles, session);
    if (serialized != null) {
      persistLocal(serialized);
    }
  }, [deferredFiles, session]);
}

export type FileId = number;

export interface PlaygroundFile {
  id: FileId;
  name: string;
  uri: Readonly<Uri>;
  handle: FileHandle | null;
}

export type FileMetadata = Readonly<Record<FileId, PlaygroundFile>>;

export type ReadonlyFiles = Readonly<FilesState>;

interface FilesState {
  /**
   * The currently selected file that is shown in the editor.
   */
  selected: FileId | null;

  /**
   * The files in display order (ordering is sensitive)
   */
  order: ReadonlyArray<FileId>;

  /**
   * File metadata by file id.
   */
  metadata: FileMetadata;

  /**
   * Invalidation token for file metadata changes and document content changes.
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
      uri: Readonly<Uri>;
    }
  | {
      type: "rename";
      id: FileId;
      to: string;
      newUri: Readonly<Uri>;
      newHandle: FileHandle | null;
    }
  | {
      type: "remove";
      id: FileId;
    }
  | { type: "selectFile"; id: FileId }
  | { type: "selectFileByName"; name: string }
  | { type: "documentChanged" }
  | { type: "reset" }
  | {
      type: "selectVendoredFile";
      handle: FileHandle;
    }
  | { type: "clearVendoredFile" };

const INIT_FILES_STATE: ReadonlyFiles = {
  order: [],
  metadata: Object.create(null),
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
      const { handle, name, uri } = action;
      const id = state.nextId;
      return {
        ...state,
        selected: id,
        order: [...state.order, id],
        metadata: { ...state.metadata, [id]: { id, name, uri, handle } },
        nextId: state.nextId + 1,
        revision: state.revision + 1,
        currentVendoredFile: null, // Clear vendored file when adding new file
      };
    }

    case "remove": {
      const { id } = action;

      let selected = state.selected;

      if (state.selected === id) {
        const position = state.order.indexOf(id);

        selected =
          (position > 0
            ? state.order[position - 1]
            : state.order[position + 1]) ?? null;
      }

      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { [id]: _metadata, ...metadata } = state.metadata;

      return {
        ...state,
        selected,
        order: state.order.filter((fileId) => fileId !== id),
        metadata,
        revision: state.revision + 1,
        currentVendoredFile: null, // Clear vendored file when removing file
      };
    }
    case "rename": {
      const { id, to, newUri, newHandle } = action;
      const file = state.metadata[id];

      return {
        ...state,
        metadata: {
          ...state.metadata,
          [id]: { ...file, name: to, uri: newUri, handle: newHandle },
        },
        revision: state.revision + 1,
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

      const selected = state.order.find(
        (id) => state.metadata[id].name === name,
      );

      return {
        ...state,
        selected: selected ?? null,
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

    case "documentChanged": {
      return {
        ...state,
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
  session: PlaygroundSession | null,
): SerializedFiles | null {
  if (session == null) {
    return null;
  }

  const serializedFiles = Object.create(null);
  let selected = null;

  for (const id of files.order) {
    const { name } = files.metadata[id];
    const text = session.text(name);
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

/**
 * Owns the mutable playground state: the ty workspace, Monaco models, and the
 * synchronization from Monaco document edits into the workspace. Immutable UI
 * metadata, like file names and selection, lives in `FilesState`.
 */
export class PlaygroundSession {
  constructor(
    private monaco: Monaco,
    readonly workspace: Workspace,
    private setError: (error: string | null) => void,
    private onChanged: () => void,
  ) {}

  openDocument(
    name: string,
    content: string,
    handle: FileHandle | null,
  ): editor.ITextModel {
    if (this.model(name) != null) {
      throw new Error(`Document ${name} is already open`);
    }

    const model = this.monaco.editor.createModel(
      content,
      languageForFile(handle ?? name),
      this.monaco.Uri.file(name),
    );
    this.registerModelChanged(name, handle, model);

    return model;
  }

  renameDocument(
    oldName: string,
    newName: string,
    newHandle: FileHandle | null,
  ): editor.ITextModel {
    const content = this.text(oldName) ?? "";
    this.closeDocument(oldName);
    return this.openDocument(newName, content, newHandle);
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

  private registerModelChanged(
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
    return this.monaco.editor.getModel(this.monaco.Uri.file(name));
  }
}

function languageForFile(file: FileHandle | string): string | undefined {
  if (typeof file === "string") {
    return file.endsWith(".py") ||
      file.endsWith(".pyi") ||
      file.endsWith(".pyw")
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
  session: PlaygroundSession,
  state: {
    files: { [name: string]: string };
    current: string;
  },
  dispatchFiles: ActionDispatch<[FileAction]>,
  setError: (error: string | null) => void,
) {
  const workspace = session.workspace;
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

    const model = session.openDocument(name, content, handle);
    dispatchFiles({
      type: "add",
      handle,
      name,
      uri: model.uri,
    });
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

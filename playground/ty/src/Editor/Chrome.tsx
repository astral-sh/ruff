import {
  lazy,
  use,
  useCallback,
  useDeferredValue,
  useMemo,
  useRef,
  useState,
} from "react";
import classNames from "classnames";
import {
  ErrorMessage,
  HorizontalResizeHandle,
  Theme,
  VerticalResizeHandle,
} from "shared";
import { FileHandle, Workspace } from "ty_wasm";
import {
  Panel,
  Group as PanelGroup,
  useDefaultLayout,
} from "react-resizable-panels";
import { Files, isPythonFile } from "./Files";
import SecondarySideBar from "./SecondarySideBar";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import Diagnostics, { Diagnostic } from "./Diagnostics";
import VendoredFileBanner from "./VendoredFileBanner";
import { FileId, ReadonlyFiles, SETTINGS_FILE_NAME } from "../Playground";
import {
  formatError,
  type InstallationStatus,
  type PackageKind,
} from "./PackageInstaller";
import type { editor } from "monaco-editor";
import type { Monaco } from "@monaco-editor/react";

const Editor = lazy(() => import("./Editor"));

interface CheckResult {
  diagnostics: Diagnostic[];
  error: string | null;
  secondary: SecondaryPanelResult;
}

export interface Props {
  workspacePromise: Promise<Workspace>;
  files: ReadonlyFiles;
  theme: Theme;
  selectedFileName: string;
  installStatus: InstallationStatus;

  onAddFile(workspace: Workspace, name: string): void;

  onChangeFile(workspace: Workspace, content: string): void;

  onRenameFile(workspace: Workspace, file: FileId, newName: string): void;

  onRemoveFile(workspace: Workspace, file: FileId): void;

  onSelectFile(id: FileId): void;

  onSelectVendoredFile(handle: FileHandle): void;

  onClearVendoredFile(): void;

  onInstallDependencies(): void;
}

export default function Chrome({
  files,
  selectedFileName,
  workspacePromise,
  theme,
  installStatus,
  onAddFile,
  onRenameFile,
  onRemoveFile,
  onSelectFile,
  onChangeFile,
  onSelectVendoredFile,
  onClearVendoredFile,
  onInstallDependencies,
}: Props) {
  const workspace = use(workspacePromise);

  const [secondaryTool, setSecondaryTool] = useState<SecondaryTool | null>(
    null,
  );

  const editorRef = useRef<{
    editor: editor.IStandaloneCodeEditor;
    monaco: Monaco;
  } | null>(null);

  const handleFileRenamed = (file: FileId, newName: string) => {
    onRenameFile(workspace, file, newName);
    editorRef.current?.editor.focus();
  };

  const handleBackToUserFile = useCallback(() => {
    if (editorRef.current && files.selected != null) {
      const selectedFile = files.index.find(
        (file) => file.id === files.selected,
      );
      if (selectedFile != null) {
        const monaco = editorRef.current.monaco;
        const fileUri = monaco.Uri.file(selectedFile.name);
        const userModel = monaco.editor.getModel(fileUri);

        if (userModel != null) {
          onClearVendoredFile();
          editorRef.current.editor.setModel(userModel);
        }
      }
    }
  }, [files.selected, files.index, onClearVendoredFile]);

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

  const handleEditorMount = useCallback(
    (editor: editor.IStandaloneCodeEditor, monaco: Monaco) => {
      editorRef.current = { editor, monaco };
    },
    [],
  );

  const handleGoTo = useCallback((line: number, column: number) => {
    const editor = editorRef.current?.editor;

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

  const handleRemoved = useCallback(
    async (id: FileId) => {
      const name = files.index.find((file) => file.id === id)?.name;

      if (name != null && editorRef.current != null) {
        // Remove the file from the monaco state to avoid that monaco "restores" the old content.
        // An alternative is to use a `key` on the `Editor` but that means we lose focus and selection
        // range when changing between tabs.
        const monaco = await import("monaco-editor");
        editorRef.current.monaco.editor
          .getModel(monaco.Uri.file(name))
          ?.dispose();
      }

      onRemoveFile(workspace, id);
    },
    [workspace, files.index, onRemoveFile],
  );

  const handleChange = useCallback(
    (content: string) => {
      onChangeFile(workspace, content);
    },
    [onChangeFile, workspace],
  );

  const { defaultLayout, onLayoutChange } = useDefaultLayout({
    groupId: "editor-diagnostics",
    storage: localStorage,
  });

  const checkResult = useCheckResult(
    files,
    workspace,
    secondaryTool,
    files.currentVendoredFile ?? null,
  );

  return (
    <>
      {files.selected != null ? (
        <>
          <Files
            files={files.index}
            theme={theme}
            selected={files.selected}
            onAdd={(name) => onAddFile(workspace, name)}
            onRename={handleFileRenamed}
            onSelect={onSelectFile}
            onRemove={handleRemoved}
          />
          <PanelGroup
            id="main-group"
            orientation="horizontal"
            className="h-full"
          >
            <Panel id="main" minSize={100}>
              <PanelGroup
                id="editor-diagnostics"
                orientation="vertical"
                className="h-full"
                defaultLayout={defaultLayout}
                onLayoutChange={onLayoutChange}
              >
                <Panel id="editor" minSize={100}>
                  {files.currentVendoredFile != null && (
                    <VendoredFileBanner
                      currentVendoredFile={files.currentVendoredFile}
                      selectedFile={{
                        id: files.selected,
                        name: selectedFileName,
                      }}
                      onBackToUserFile={handleBackToUserFile}
                    />
                  )}
                  <Editor
                    theme={theme}
                    visible={true}
                    files={files}
                    selected={files.selected}
                    fileName={selectedFileName}
                    diagnostics={checkResult.diagnostics}
                    workspace={workspace}
                    onMount={handleEditorMount}
                    onChange={handleChange}
                    onOpenFile={onSelectFile}
                    onVendoredFileChange={onSelectVendoredFile}
                    onBackToUserFile={handleBackToUserFile}
                    isViewingVendoredFile={files.currentVendoredFile != null}
                  />
                  {checkResult.error ? (
                    <div
                      style={{
                        position: "fixed",
                        left: "10%",
                        right: "10%",
                        bottom: "10%",
                      }}
                    >
                      <ErrorMessage>{checkResult.error}</ErrorMessage>
                    </div>
                  ) : null}
                </Panel>
                <VerticalResizeHandle />
                <Panel id="diagnostics" minSize={150} className="my-2">
                  {selectedFileName === SETTINGS_FILE_NAME ? (
                    <DependenciesPanel
                      theme={theme}
                      status={installStatus}
                      onInstall={onInstallDependencies}
                    />
                  ) : (
                    <Diagnostics
                      diagnostics={checkResult.diagnostics}
                      onGoTo={handleGoTo}
                      theme={theme}
                    />
                  )}
                </Panel>
              </PanelGroup>
            </Panel>
            {secondaryTool != null && (
              <>
                <HorizontalResizeHandle />
                <Panel id="secondary-panel" minSize={100}>
                  <SecondaryPanel
                    files={files}
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
    </>
  );
}

function useCheckResult(
  files: ReadonlyFiles,
  workspace: Workspace,
  secondaryTool: SecondaryTool | null,
  currentVendoredFileHandle: FileHandle | null,
): CheckResult {
  const deferredContent = useDeferredValue(
    files.selected == null ? null : files.contents[files.selected],
  );

  return useMemo(() => {
    // Determine which file handle to use
    const currentHandle =
      currentVendoredFileHandle ??
      (files.selected == null ? null : files.handles[files.selected]);

    const isVendoredFile = currentVendoredFileHandle != null;

    // Regular file handling
    if (
      currentHandle == null ||
      deferredContent == null ||
      !isPythonFile(currentHandle)
    ) {
      return {
        diagnostics: [],
        error: null,
        secondary: null,
      };
    }

    try {
      // Don't run diagnostics for vendored files - always empty
      const diagnostics = isVendoredFile
        ? []
        : workspace.checkFile(currentHandle);

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

          case "Run":
            secondary = isVendoredFile
              ? {
                  status: "error",
                  error: "Cannot run vendored/standard library files",
                }
              : {
                  status: "ok",
                  content: "",
                };
            break;
        }
      } catch (error: unknown) {
        secondary = {
          status: "error",
          error: error instanceof Error ? error.message : error + "",
        };
      }

      // Convert diagnostics (empty array for vendored files)
      const serializedDiagnostics = diagnostics.map((diagnostic) => ({
        id: diagnostic.id(),
        message: diagnostic.message(),
        severity: diagnostic.severity(),
        range: diagnostic.toRange(workspace) ?? null,
        textRange: diagnostic.textRange() ?? null,
        raw: diagnostic,
      }));

      return {
        diagnostics: serializedDiagnostics,
        error: null,
        secondary,
      };
    } catch (e) {
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
    currentVendoredFileHandle,
  ]);
}

function DependenciesPanel({
  theme,
  status,
  onInstall,
}: {
  theme: Theme;
  status: InstallationStatus;
  onInstall: () => void;
}) {
  const isInstalling = status.state === "installing";

  return (
    <div
      className={classNames(
        "flex h-full flex-col overflow-hidden",
        theme === "dark" ? "text-white" : null,
      )}
    >
      <div
        className={classNames(
          "shrink-0 border-b border-gray-200 px-2 py-1 flex items-center",
          theme === "dark" ? "border-rock" : null,
        )}
      >
        <span>Dependencies</span>
        <button
          onClick={onInstall}
          disabled={isInstalling}
          className={classNames(
            "ml-auto px-3 py-0.5 text-xs font-medium rounded-md cursor-pointer",
            "transition-all duration-200 uppercase tracking-[.08em]",
            isInstalling
              ? "opacity-50 cursor-not-allowed bg-radiate/60 text-black"
              : "bg-radiate text-black hover:bg-galaxy hover:text-white",
          )}
        >
          {isInstalling ? "Installing..." : "Install"}
        </button>
      </div>

      {status.state === "installing" && (
        <div className="shrink-0 flex flex-col gap-1 px-2 pt-2">
          <div className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
            <span>{status.message}</span>
            {status.progress != null && <span>{status.progress}%</span>}
          </div>
          <div className="h-1 rounded-full bg-gray-200 dark:bg-gray-700 overflow-hidden">
            {status.progress != null ? (
              <div
                className="h-full rounded-full bg-radiate transition-[width] duration-300"
                style={{ width: `${status.progress}%` }}
              />
            ) : (
              <div className="h-full w-1/3 rounded-full bg-radiate animate-[indeterminate_1.5s_ease-in-out_infinite]" />
            )}
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-2">
        <DependenciesContent status={status} theme={theme} />
      </div>
    </div>
  );
}

function DependenciesContent({
  status,
  theme,
}: {
  status: InstallationStatus;
  theme: Theme;
}) {
  if (status.state === "error") {
    return (
      <div className="flex flex-auto flex-col justify-center items-center gap-1 px-4 text-center">
        <span className="text-red-500 dark:text-red-400 text-sm">
          {status.message}
        </span>
      </div>
    );
  }

  const packages = status.installedPackages;

  if (packages.length > 0) {
    return (
      <div className="space-y-2">
        {status.warnings.length > 0 && (
          <ul className="space-y-0.5">
            {status.warnings.map((warning, i) => (
              <li key={i} className="text-xs text-ayu-accent select-text">
                {warning}
              </li>
            ))}
          </ul>
        )}
        <ul className="space-y-0.5">
          {packages.map((pkg) => (
            <li
              key={pkg.name}
              className="flex items-baseline gap-2 text-sm select-text"
            >
              <span>{pkg.name}</span>
              <span
                className={classNames(
                  "text-xs",
                  theme === "dark" ? "text-gray-500" : "text-gray-400",
                )}
              >
                {pkg.version}
              </span>
              <PackageKindBadge kind={pkg.kind} stubsSource={pkg.stubsSource} />
            </li>
          ))}
        </ul>
      </div>
    );
  }

  return (
    <div className="flex flex-auto flex-col justify-center items-center text-gray-400 dark:text-gray-500 text-sm">
      Add packages to &quot;dependencies&quot; and click Install.
    </div>
  );
}

function PackageKindBadge({
  kind,
  stubsSource,
}: {
  kind: PackageKind;
  stubsSource?: string;
}) {
  if (kind === "pure-python") {
    return null;
  }

  if (kind === "stubs-only") {
    return (
      <span
        className="inline-block px-1.5 py-0 text-[10px] font-medium rounded bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300"
        title={stubsSource ? `Stubs from ${stubsSource}` : "Type stubs only"}
      >
        stubs
      </span>
    );
  }

  return (
    <span
      className="inline-block px-1.5 py-0 text-[10px] font-medium rounded bg-yellow-100 text-yellow-700 dark:bg-yellow-900/40 dark:text-yellow-300"
      title="No type information available — runtime execution only"
    >
      runtime
    </span>
  );
}

import {
  lazy,
  use,
  useCallback,
  useDeferredValue,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ErrorMessage,
  HorizontalResizeHandle,
  Theme,
  VerticalResizeHandle,
} from "shared";
import { FileHandle, Workspace } from "ty_wasm";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Files, isPythonFile } from "./Files";
import SecondarySideBar from "./SecondarySideBar";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import Diagnostics, { Diagnostic } from "./Diagnostics";
import VendoredFileBanner from "./VendoredFileBanner";
import { FileId, ReadonlyFiles } from "../Playground";
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

  onAddFile(workspace: Workspace, name: string): void;

  onChangeFile(workspace: Workspace, content: string): void;

  onRenameFile(workspace: Workspace, file: FileId, newName: string): void;

  onRemoveFile(workspace: Workspace, file: FileId): void;

  onSelectFile(id: FileId): void;

  onSelectVendoredFile(handle: FileHandle): void;

  onClearVendoredFile(): void;
}

export default function Chrome({
  files,
  selectedFileName,
  workspacePromise,
  theme,
  onAddFile,
  onRenameFile,
  onRemoveFile,
  onSelectFile,
  onChangeFile,
  onSelectVendoredFile,
  onClearVendoredFile,
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
          <PanelGroup direction="horizontal" autoSaveId="main">
            <Panel
              id="main"
              order={0}
              className={`flex flex-col gap-2 ${files.currentVendoredFile ? "mb-4" : "my-4"}`}
              minSize={10}
            >
              <PanelGroup id="vertical" direction="vertical">
                <Panel
                  minSize={10}
                  className={files.currentVendoredFile ? "mb-2" : "my-2"}
                  order={0}
                >
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
                    onChange={(content) => onChangeFile(workspace, content)}
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
                <Panel
                  id="diagnostics"
                  minSize={3}
                  order={1}
                  className="my-2 flex grow"
                >
                  <Diagnostics
                    diagnostics={checkResult.diagnostics}
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

export function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}

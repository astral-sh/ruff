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
import type { Workspace } from "ty_wasm";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Files, isPythonFile } from "./Files";
import SecondarySideBar from "./SecondarySideBar";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import Diagnostics, { Diagnostic } from "./Diagnostics";
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
}: Props) {
  const workspace = use(workspacePromise);

  const [secondaryTool, setSecondaryTool] = useState<SecondaryTool | null>(
    null,
  );

  const [currentVendoredFile, setCurrentVendoredFile] = useState<{
    path: string;
    previousFileId: FileId | null;
  } | null>(null);


  const editorRef = useRef<{
    editor: editor.IStandaloneCodeEditor;
    monaco: Monaco;
  } | null>(null);

  const handleFileRenamed = (file: FileId, newName: string) => {
    onRenameFile(workspace, file, newName);
    editorRef.current?.editor.focus();
  };

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

  const handleVendoredFileChange = useCallback(
    (vendoredPath: string | null, previousFileId: FileId | null) => {
      if (vendoredPath) {
        setCurrentVendoredFile(current => {
          // If we're already viewing a vendored file, preserve the original previousFileId
          const preservedPreviousFileId = current?.previousFileId ?? previousFileId;
          return { path: vendoredPath, previousFileId: preservedPreviousFileId };
        });
      } else {
        setCurrentVendoredFile(null);
      }
    },
    [],
  );

  const checkResult = useCheckResult(files, workspace, secondaryTool);

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
              className={`flex flex-col gap-2 ${currentVendoredFile ? "mb-4" : "my-4"}`}
              minSize={10}
            >
              <PanelGroup id="vertical" direction="vertical">
                <Panel minSize={10} className={currentVendoredFile ? "mb-2" : "my-2"} order={0}>
                  {currentVendoredFile && (
                    <div className="bg-blue-50 dark:bg-blue-900 px-3 py-2 border-b border-blue-200 dark:border-blue-700 text-sm">
                      <div className="flex items-center justify-between">
                        <div>
                          <span className="font-medium text-blue-800 dark:text-blue-200">Viewing standard library file:</span>{" "}
                          <code className="font-mono text-blue-700 dark:text-blue-300">{currentVendoredFile.path}</code>
                          <span className="text-blue-600 dark:text-blue-400 ml-2 text-xs">
                            (read-only)
                          </span>
                        </div>
                        <button
                          onClick={() => {
                            
                            if (currentVendoredFile?.previousFileId != null) {
                              
                              // Find the previous file
                              const previousFile = files.index.find(f => f.id === currentVendoredFile.previousFileId);
                              
                              if (previousFile) {
                                
                                // Force the editor to switch back to the user file immediately
                                if (editorRef.current) {
                                  const monaco = editorRef.current.monaco;
                                  const fileUri = monaco.Uri.file(previousFile.name);
                                  
                                  
                                  // Get or create the model for the user file
                                  let userModel = monaco.editor.getModel(fileUri);
                                  if (!userModel) {
                                    userModel = monaco.editor.createModel(
                                      files.contents[currentVendoredFile.previousFileId] || '',
                                      'python',
                                      fileUri
                                    );
                                  } else {
                                    // Update the model content
                                    userModel.setValue(files.contents[currentVendoredFile.previousFileId] || '');
                                  }
                                  
                                  // Force the editor to use this model
                                  editorRef.current.editor.setModel(userModel);
                                }
                                
                                // Clear vendored file state AFTER forcing the editor change
                                setCurrentVendoredFile(null);
                                
                                // Update the application state
                                onSelectFile(currentVendoredFile.previousFileId);
                              }
                            } else {
                              setCurrentVendoredFile(null);
                            }
                          }}
                          className="px-3 py-1 text-xs bg-blue-100 dark:bg-blue-800 text-blue-800 dark:text-blue-200 rounded border border-blue-300 dark:border-blue-600 hover:bg-blue-200 dark:hover:bg-blue-700 transition-colors"
                        >
                          Back to {currentVendoredFile.previousFileId ? files.index.find(f => f.id === currentVendoredFile.previousFileId)?.name || 'file' : 'editor'}
                        </button>
                      </div>
                    </div>
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
                    onVendoredFileChange={handleVendoredFileChange}
                    isViewingVendoredFile={currentVendoredFile != null}
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
): CheckResult {
  const deferredContent = useDeferredValue(
    files.selected == null ? null : files.contents[files.selected],
  );

  return useMemo(() => {
    if (files.selected == null || deferredContent == null) {
      return {
        diagnostics: [],
        error: null,
        secondary: null,
      };
    }

    const currentHandle = files.handles[files.selected];
    if (currentHandle == null || !isPythonFile(currentHandle)) {
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

          case "Run":
            secondary = {
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

      // Eagerly convert the diagnostic to avoid out of bound errors
      // when the diagnostics are "deferred".
      const serializedDiagnostics = diagnostics.map((diagnostic) => ({
        id: diagnostic.id(),
        message: diagnostic.message(),
        severity: diagnostic.severity(),
        range: diagnostic.toRange(workspace) ?? null,
        textRange: diagnostic.textRange() ?? null,
      }));

      return {
        diagnostics: serializedDiagnostics,
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

export function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}

/**
 * Editor for the Python source code.
 */

import Moncao, { Monaco, OnMount } from "@monaco-editor/react";
import { editor, MarkerSeverity } from "monaco-editor";
import { useCallback, useEffect, useRef } from "react";
import { Theme } from "shared";
import { Diagnostic, Severity, Workspace } from "red_knot_wasm";

import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

type Props = {
  visible: boolean;
  fileName: string;
  source: string;
  diagnostics: Diagnostic[];
  theme: Theme;
  workspace: Workspace;
  onChange(content: string): void;
  onMount(editor: IStandaloneCodeEditor, monaco: Monaco): void;
};

type MonacoEditorState = {
  monaco: Monaco;
};

export default function Editor({
  visible,
  source,
  fileName,
  theme,
  diagnostics,
  workspace,
  onChange,
  onMount,
}: Props) {
  const monacoRef = useRef<MonacoEditorState | null>(null);

  // Update the diagnostics in the editor.
  useEffect(() => {
    const editorState = monacoRef.current;

    if (editorState == null) {
      return;
    }

    updateMarkers(editorState.monaco, workspace, diagnostics);
  }, [workspace, diagnostics]);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      updateMarkers(instance, workspace, diagnostics);

      monacoRef.current = {
        monaco: instance,
      };

      onMount(editor, instance);
    },

    [onMount, workspace, diagnostics],
  );

  return (
    <Moncao
      onMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: false,
      }}
      language={fileName.endsWith(".pyi") ? "python" : undefined}
      path={fileName}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={source}
      onChange={handleChange}
    />
  );
}

function updateMarkers(
  monaco: Monaco,
  workspace: Workspace,
  diagnostics: Array<Diagnostic>,
) {
  const editor = monaco.editor;
  const model = editor?.getModels()[0];

  if (!model) {
    return;
  }

  editor.setModelMarkers(
    model,
    "owner",
    diagnostics.map((diagnostic) => {
      const mapSeverity = (severity: Severity) => {
        switch (severity) {
          case Severity.Info:
            return MarkerSeverity.Info;
          case Severity.Warning:
            return MarkerSeverity.Warning;
          case Severity.Error:
            return MarkerSeverity.Error;
          case Severity.Fatal:
            return MarkerSeverity.Error;
        }
      };

      const range = diagnostic.toRange(workspace);

      return {
        code: diagnostic.id(),
        startLineNumber: range?.start?.line ?? 0,
        startColumn: range?.start?.column ?? 0,
        endLineNumber: range?.end?.line ?? 0,
        endColumn: range?.end?.column ?? 0,
        message: diagnostic.message(),
        severity: mapSeverity(diagnostic.severity()),
        tags: [],
      };
    }),
  );
}

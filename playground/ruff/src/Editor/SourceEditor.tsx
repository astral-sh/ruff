/**
 * Editor for the Python source code.
 */

import MonacoEditor, { Monaco, OnMount } from "@monaco-editor/react";
import {
  editor,
  IDisposable,
  languages,
  MarkerSeverity,
  MarkerTag,
  Range,
} from "monaco-editor";
import { useCallback, useEffect, useRef } from "react";
import { Diagnostic } from "ruff_wasm";
import { Theme } from "shared";
import CodeActionProvider = languages.CodeActionProvider;
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

type MonacoEditorState = {
  monaco: Monaco;
  codeActionProvider: RuffCodeActionProvider;
  disposeCodeActionProvider: IDisposable;
};

export default function SourceEditor({
  visible,
  source,
  theme,
  diagnostics,
  onChange,
  onMount,
}: {
  visible: boolean;
  source: string;
  diagnostics: Diagnostic[];
  theme: Theme;
  onChange(pythonSource: string): void;
  onMount(editor: IStandaloneCodeEditor): void;
}) {
  const monacoRef = useRef<MonacoEditorState | null>(null);

  // Update the diagnostics in the editor.
  useEffect(() => {
    const editorState = monacoRef.current;

    if (editorState == null) {
      return;
    }

    editorState.codeActionProvider.diagnostics = diagnostics;

    updateMarkers(editorState.monaco, diagnostics);
  }, [diagnostics]);

  // Dispose the code action provider on unmount.
  useEffect(() => {
    const disposeActionProvider = monacoRef.current?.disposeCodeActionProvider;
    if (disposeActionProvider == null) {
      return;
    }

    return () => {
      disposeActionProvider.dispose();
    };
  }, []);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      const ruffActionsProvider = new RuffCodeActionProvider(diagnostics);
      const disposeCodeActionProvider =
        instance.languages.registerCodeActionProvider(
          "python",
          ruffActionsProvider,
        );

      updateMarkers(instance, diagnostics);

      monacoRef.current = {
        monaco: instance,
        codeActionProvider: ruffActionsProvider,
        disposeCodeActionProvider,
      };

      onMount(editor);
    },

    [diagnostics, onMount],
  );

  return (
    <MonacoEditor
      onMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: true,
      }}
      language={"python"}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={source}
      onChange={handleChange}
    />
  );
}

class RuffCodeActionProvider implements CodeActionProvider {
  constructor(public diagnostics: Array<Diagnostic>) {}

  provideCodeActions(
    model: editor.ITextModel,
    range: Range,
  ): languages.ProviderResult<languages.CodeActionList> {
    const actions = this.diagnostics
      .filter((check) => range.startLineNumber === check.location.row)
      .filter(({ fix }) => fix)
      .map((check) => ({
        title: check.fix
          ? check.fix.message
            ? `${check.code}: ${check.fix.message}`
            : `Fix ${check.code}`
          : "Fix",
        id: `fix-${check.code}`,
        kind: "quickfix",

        edit: check.fix
          ? {
              edits: check.fix.edits.map((edit) => ({
                resource: model.uri,
                versionId: model.getVersionId(),
                textEdit: {
                  range: {
                    startLineNumber: edit.location.row,
                    startColumn: edit.location.column,
                    endLineNumber: edit.end_location.row,
                    endColumn: edit.end_location.column,
                  },
                  text: edit.content || "",
                },
              })),
            }
          : undefined,
      }));

    return {
      actions,
      dispose: () => {},
    };
  }
}

function updateMarkers(monaco: Monaco, diagnostics: Array<Diagnostic>) {
  const editor = monaco.editor;
  const model = editor?.getModels()[0];

  if (!model) {
    return;
  }

  editor.setModelMarkers(
    model,
    "owner",
    diagnostics.map((diagnostic) => ({
      startLineNumber: diagnostic.location.row,
      startColumn: diagnostic.location.column,
      endLineNumber: diagnostic.end_location.row,
      endColumn: diagnostic.end_location.column,
      message: diagnostic.code
        ? `${diagnostic.code}: ${diagnostic.message}`
        : diagnostic.message,
      severity: MarkerSeverity.Error,
      tags:
        diagnostic.code === "F401" || diagnostic.code === "F841"
          ? [MarkerTag.Unnecessary]
          : [],
    })),
  );
}

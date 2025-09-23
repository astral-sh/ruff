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
    // Convert UTF-32 (code points) columns from WASM to Monaco's UTF-16 columns
    const utf32ToUtf16 = (lineText: string, utf32Column: number): number => {
      // Monaco columns are 1-based. utf32Column is also 1-based (OneIndexed).
      if (utf32Column <= 1) return 1;
      const prefix = [...lineText].slice(0, utf32Column - 1).join("");
      return prefix.length + 1; // length in UTF-16 code units, then make 1-based
    };
    const actions = this.diagnostics
      // Show fixes for any diagnostic whose range intersects the requested range
      .filter((check) =>
        Range.areIntersecting(
          new Range(
            check.start_location.row,
            check.start_location.column,
            check.end_location.row,
            check.end_location.column,
          ),
          range,
        ),
      )
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
                    startColumn: utf32ToUtf16(
                      model.getLineContent(edit.location.row),
                      edit.location.column,
                    ),
                    endLineNumber: edit.end_location.row,
                    endColumn: utf32ToUtf16(
                      model.getLineContent(edit.end_location.row),
                      edit.end_location.column,
                    ),
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

  // Helper to convert UTF-32 (code points) columns from WASM to Monaco's UTF-16 columns
  const utf32ToUtf16 = (lineText: string, utf32Column: number): number => {
    if (utf32Column <= 1) return 1;
    const prefix = [...lineText].slice(0, utf32Column - 1).join("");
    return prefix.length + 1;
  };

  editor.setModelMarkers(
    model,
    "owner",
    diagnostics.map((diagnostic) => ({
      code: diagnostic.code ?? undefined,
      startLineNumber: diagnostic.start_location.row,
      startColumn: utf32ToUtf16(
        model.getLineContent(diagnostic.start_location.row),
        diagnostic.start_location.column,
      ),
      endLineNumber: diagnostic.end_location.row,
      endColumn: utf32ToUtf16(
        model.getLineContent(diagnostic.end_location.row),
        diagnostic.end_location.column,
      ),
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

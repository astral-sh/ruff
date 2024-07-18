/**
 * Editor for the Python source code.
 */

import Editor, { BeforeMount, Monaco } from "@monaco-editor/react";
import { MarkerSeverity, MarkerTag } from "monaco-editor";
import { useCallback, useEffect, useRef } from "react";
import { Diagnostic } from "../pkg";
import { Theme } from "./theme";

export default function SourceEditor({
  visible,
  source,
  theme,
  diagnostics,
  onChange,
}: {
  visible: boolean;
  source: string;
  diagnostics: Diagnostic[];
  theme: Theme;
  onChange: (pythonSource: string) => void;
}) {
  const monacoRef = useRef<Monaco | null>(null);
  const monaco = monacoRef.current;

  useEffect(() => {
    const editor = monaco?.editor;
    const model = editor?.getModels()[0];
    if (!editor || !model) {
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

    const codeActionProvider = monaco?.languages.registerCodeActionProvider(
      "python",
      {
        provideCodeActions: function (model, position) {
          const actions = diagnostics
            .filter((check) => position.startLineNumber === check.location.row)
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
          return { actions, dispose: () => {} };
        },
      },
    );

    return () => {
      codeActionProvider?.dispose();
    };
  }, [diagnostics, monaco]);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  const handleMount: BeforeMount = useCallback(
    (instance) => (monacoRef.current = instance),
    [],
  );

  return (
    <Editor
      beforeMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: false,
      }}
      language={"python"}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={source}
      onChange={handleChange}
    />
  );
}

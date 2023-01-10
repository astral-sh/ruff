/**
 * Editor for the Python source code.
 */

import Editor, { useMonaco } from "@monaco-editor/react";
import { MarkerSeverity, MarkerTag } from "monaco-editor";
import { useCallback, useEffect } from "react";
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
  const monaco = useMonaco();

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
        startColumn: diagnostic.location.column + 1,
        endLineNumber: diagnostic.end_location.row,
        endColumn: diagnostic.end_location.column + 1,
        message: `${diagnostic.code}: ${diagnostic.message}`,
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
        // @ts-expect-error: The type definition is wrong.
        provideCodeActions: function (model, position) {
          const actions = diagnostics
            .filter((check) => position.startLineNumber === check.location.row)
            .filter((check) => check.fix)
            .map((check) => ({
              title: check.fix
                ? `${check.code}: ${check.fix.message}` ?? `Fix ${check.code}`
                : "Autofix",
              id: `fix-${check.code}`,
              kind: "quickfix",
              edit: check.fix
                ? {
                    edits: [
                      {
                        resource: model.uri,
                        versionId: model.getVersionId(),
                        edit: {
                          range: {
                            startLineNumber: check.fix.location.row,
                            startColumn: check.fix.location.column + 1,
                            endLineNumber: check.fix.end_location.row,
                            endColumn: check.fix.end_location.column + 1,
                          },
                          text: check.fix.content,
                        },
                      },
                    ],
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

  return (
    <Editor
      options={{
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
      }}
      language={"python"}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={source}
      onChange={handleChange}
    />
  );
}

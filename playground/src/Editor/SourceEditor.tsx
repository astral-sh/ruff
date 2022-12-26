/**
 * Editor for the Python source code.
 */

import Editor, { useMonaco } from "@monaco-editor/react";
import { MarkerSeverity, MarkerTag } from "monaco-editor";
import { useCallback, useEffect } from "react";
import { Check } from "../pkg";

export type Mode = "JSON" | "Python";

export default function SourceEditor({
  visible,
  source,
  checks,
  onChange,
}: {
  visible: boolean;
  source: string;
  checks: Check[];
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
      checks.map((check) => ({
        startLineNumber: check.location.row,
        startColumn: check.location.column + 1,
        endLineNumber: check.end_location.row,
        endColumn: check.end_location.column + 1,
        message: `${check.code}: ${check.message}`,
        severity: MarkerSeverity.Error,
        tags:
          check.code === "F401" || check.code === "F841"
            ? [MarkerTag.Unnecessary]
            : [],
      }))
    );

    const codeActionProvider = monaco?.languages.registerCodeActionProvider(
      "python",
      {
        // @ts-expect-error: The type definition is wrong.
        provideCodeActions: function (model, position) {
          const actions = checks
            .filter((check) => position.startLineNumber === check.location.row)
            .filter((check) => check.fix)
            .map((check) => ({
              title: `Fix ${check.code}`,
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
      }
    );

    return () => {
      codeActionProvider?.dispose();
    };
  }, [checks, monaco]);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange]
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
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={"Ayu-Light"}
      language={"python"}
      value={source}
      onChange={handleChange}
    />
  );
}

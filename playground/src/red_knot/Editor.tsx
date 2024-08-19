/**
 * Editor for the Python source code.
 */

import Moncao, { BeforeMount, Monaco } from "@monaco-editor/react";
import { MarkerSeverity } from "monaco-editor";
import { useCallback, useEffect, useRef } from "react";
import { Theme } from "../shared/theme";

type Props = {
  visible: boolean;
  source: string;
  diagnostics: string[];
  theme: Theme;
  onChange(content: string): void;
};

export default function Editor({
  visible,
  source,
  theme,
  diagnostics,
  onChange,
}: Props) {
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
        startLineNumber: 1,
        startColumn: 1,
        endLineNumber: 1,
        endColumn: 1,
        message: diagnostic,
        severity: MarkerSeverity.Error,
        tags: [],
      })),
    );
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
    <Moncao
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

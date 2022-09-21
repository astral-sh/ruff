import Editor, { Monaco } from "@monaco-editor/react";
import { editor, MarkerSeverity } from "monaco-editor/esm/vs/editor/editor.api";
import React, { useCallback, useEffect, useState } from "react";
import { DEFAULT_SOURCE } from "./constants";
import init, { check } from "./pkg/ruff.js";
import { Check } from "./types";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

function App() {
  const [initialized, setInitialized] = useState<boolean>(false);
  const [_, setCounter] = useState(0);
  const [monaco, setMonaco] = useState<Monaco | null>(null);

  // Load the WASM module.
  useEffect(() => {
    init().then(() => setInitialized(true));
  }, []);

  const handleEditorChange = useCallback(
    () => setCounter((counter) => counter + 1),
    []
  );
  const handleEditorDidMount = useCallback(
    (editor: IStandaloneCodeEditor, monaco: Monaco) => {
      setMonaco(monaco);
    },
    []
  );

  if (initialized) {
    if (monaco) {
      const editor = monaco.editor;
      if (editor) {
        const model = editor.getModels()[0];
        if (model) {
          const checks: Check[] = JSON.parse(check(model.getValue()));
          editor.setModelMarkers(
            model,
            "owner",
            checks.map((check) => ({
              startLineNumber: check.location.row,
              startColumn: check.location.column,
              endLineNumber: check.location.row,
              endColumn: check.location.column,
              message: `${check.code}: ${check.message}`,
              severity: MarkerSeverity.Error,
            }))
          );
        }
      }
    }
  }

  return (
    <Editor
      height={"100%"}
      width={"100%"}
      path={"ruff"}
      options={{ readOnly: false, minimap: { enabled: false } }}
      defaultLanguage="python"
      defaultValue={DEFAULT_SOURCE}
      theme={"light"}
      onMount={handleEditorDidMount}
      onChange={handleEditorChange}
    />
  );
}

export default App;

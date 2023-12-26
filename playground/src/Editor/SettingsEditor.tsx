/**
 * Editor for the settings JSON.
 */

import Editor, { useMonaco } from "@monaco-editor/react";
import { useCallback, useEffect } from "react";
import schema from "../../../ruff.schema.json";
import { Theme } from "./theme";

export default function SettingsEditor({
  visible,
  source,
  theme,
  onChange,
}: {
  visible: boolean;
  source: string;
  theme: Theme;
  onChange: (source: string) => void;
}) {
  const monaco = useMonaco();

  useEffect(() => {
    monaco?.languages.json.jsonDefaults.setDiagnosticsOptions({
      schemas: [
        {
          uri: "https://raw.githubusercontent.com/astral-sh/ruff/main/ruff.schema.json",
          fileMatch: ["*"],
          schema,
        },
      ],
    });
  }, [monaco]);

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
        contextmenu: false,
      }}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      language={"json"}
      value={source}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      onChange={handleChange}
    />
  );
}

/**
 * Editor for the settings JSON.
 */

import { useCallback } from "react";
import { Theme } from "shared";
import MonacoEditor from "@monaco-editor/react";
import { editor } from "monaco-editor";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

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
  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  const handleMount = useCallback((editor: IStandaloneCodeEditor) => {
    editor.addAction({
      id: "copyAsRuffToml",
      label: "Copy as ruff.toml",
      contextMenuGroupId: "9_cutcopypaste",
      contextMenuOrder: 3,

      async run(editor): Promise<undefined> {
        const model = editor.getModel();

        if (model == null) {
          return;
        }

        const toml = await import("smol-toml");
        const settings = model.getValue();
        const tomlSettings = toml.stringify(JSON.parse(settings));

        await navigator.clipboard.writeText(tomlSettings);
      },
    });

    editor.addAction({
      id: "copyAsPyproject.toml",
      label: "Copy as pyproject.toml",
      contextMenuGroupId: "9_cutcopypaste",
      contextMenuOrder: 4,

      async run(editor): Promise<undefined> {
        const model = editor.getModel();

        if (model == null) {
          return;
        }

        const settings = model.getValue();
        const toml = await import("smol-toml");
        const tomlSettings = toml.stringify(
          prefixWithRuffToml(JSON.parse(settings)),
        );

        await navigator.clipboard.writeText(tomlSettings);
      },
    });
    const didPaste = editor.onDidPaste((event) => {
      const model = editor.getModel();

      if (model == null) {
        return;
      }

      // Allow pasting a TOML settings configuration if it replaces the entire settings.
      if (model.getFullModelRange().equalsRange(event.range)) {
        const pasted = model.getValueInRange(event.range);

        // Text starting with a `{` must be JSON. Don't even try to parse as TOML.
        if (!pasted.trimStart().startsWith("{")) {
          import("smol-toml").then((toml) => {
            try {
              const parsed = toml.parse(pasted);
              const cleansed = stripToolRuff(parsed);

              model.setValue(JSON.stringify(cleansed, null, 4));
            } catch (e) {
              // Turned out to not be TOML after all.
              // eslint-disable-next-line no-console
              console.warn("Failed to parse settings as TOML", e);
            }
          });
        }
      }
    });

    return () => didPaste.dispose();
  }, []);

  return (
    <MonacoEditor
      options={{
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: true,
      }}
      onMount={handleMount}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      language="json"
      value={source}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      onChange={handleChange}
    />
  );
}

function stripToolRuff(settings: object) {
  const { tool, ...nonToolSettings } = settings as any;

  // Flatten out `tool.ruff.x` to just `x`
  if (typeof tool === "object" && !Array.isArray(tool)) {
    if (tool.ruff != null) {
      return { ...nonToolSettings, ...tool.ruff };
    }
  }

  return Object.fromEntries(
    Object.entries(settings).flatMap(([key, value]) => {
      if (key.startsWith("tool.ruff")) {
        const strippedKey = key.substring("tool.ruff".length);

        if (strippedKey === "") {
          return Object.entries(value);
        }

        return [[strippedKey.substring(1), value]];
      }

      return [[key, value]];
    }),
  );
}

function prefixWithRuffToml(settings: object) {
  const subTableEntries = [];
  const ruffTableEntries = [];

  for (const [key, value] of Object.entries(settings)) {
    if (typeof value === "object" && !Array.isArray(value)) {
      subTableEntries.push([`tool.ruff.${key}`, value]);
    } else {
      ruffTableEntries.push([key, value]);
    }
  }

  return {
    ["tool.ruff"]: Object.fromEntries(ruffTableEntries),
    ...Object.fromEntries(subTableEntries),
  };
}

import lzstring from "lz-string";
import Editor, { useMonaco } from "@monaco-editor/react";
import { MarkerSeverity } from "monaco-editor/esm/vs/editor/editor.api";
import { useEffect, useState, useCallback } from "react";

import init, { Check, check } from "./pkg/ruff.js";
import { AVAILABLE_OPTIONS } from "./ruff_options";
import { Config, getDefaultConfig, toRuffConfig } from "./config";
import { Options } from "./Options";

const DEFAULT_SOURCE =
  "# Define a function that takes an integer n and returns the nth number in the Fibonacci\n" +
  "# sequence.\n" +
  "def fibonacci(n):\n" +
  "  if n == 0:\n" +
  "    return 0\n" +
  "  elif n == 1:\n" +
  "    return 1\n" +
  "  else:\n" +
  "    return fibonacci(n-1) + fibonacci(n-2)\n" +
  "\n" +
  "# Use a for loop to generate and print the first 10 numbers in the Fibonacci sequence.\n" +
  "for i in range(10):\n" +
  "  print(fibonacci(i))\n" +
  "\n" +
  "# Output:\n" +
  "# 0\n" +
  "# 1\n" +
  "# 1\n" +
  "# 2\n" +
  "# 3\n" +
  "# 5\n" +
  "# 8\n" +
  "# 13\n" +
  "# 21\n" +
  "# 34\n";

function restoreConfigAndSource(): [Config, string] {
  const value = lzstring.decompressFromEncodedURIComponent(
    window.location.hash.slice(1)
  );
  let config = {};
  let source = DEFAULT_SOURCE;

  if (value) {
    const parts = value.split("$$$");
    config = JSON.parse(parts[0]);
    source = parts[1];
  }

  return [config, source];
}

function persistConfigAndSource(config: Config, source: string) {
  window.location.hash = lzstring.compressToEncodedURIComponent(
    JSON.stringify(config) + "$$$" + source
  );
}

const defaultConfig = getDefaultConfig(AVAILABLE_OPTIONS);

export default function App() {
  const monaco = useMonaco();
  const [initialized, setInitialized] = useState<boolean>(false);
  const [config, setConfig] = useState<Config | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    init().then(() => setInitialized(true));
  }, []);

  useEffect(() => {
    if (source == null && config == null && monaco) {
      const [config, source] = restoreConfigAndSource();
      setConfig(config);
      setSource(source);
    }
  }, [monaco, source, config]);

  useEffect(() => {
    if (config != null && source != null) {
      persistConfigAndSource(config, source);
    }
  }, [config, source]);

  useEffect(() => {
    const editor = monaco?.editor;
    const model = editor?.getModels()[0];
    if (!editor || !model || !initialized || source == null || config == null) {
      return;
    }

    let checks: Check[];
    try {
      checks = check(source, toRuffConfig(config));
      setError(null);
    } catch (e) {
      setError(String(e));
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
  }, [config, source, monaco, initialized]);

  const handleEditorChange = useCallback(
    (value: string | undefined) => {
      setSource(value || "");
    },
    [setSource]
  );

  const handleOptionChange = useCallback(
    (groupName: string, fieldName: string, value: string) => {
      const group = Object.assign({}, (config || {})[groupName]);
      if (value === defaultConfig[groupName][fieldName] || value === "") {
        delete group[fieldName];
      } else {
        group[fieldName] = value;
      }

      setConfig({
        ...config,
        [groupName]: group,
      });
    },
    [config]
  );

  return (
    <div id="app">
      <Options
        config={config}
        defaultConfig={defaultConfig}
        onChange={handleOptionChange}
      />
      <Editor
        options={{ readOnly: false, minimap: { enabled: false } }}
        wrapperProps={{ className: "editor" }}
        defaultLanguage="python"
        value={source || ""}
        theme={"light"}
        onChange={handleEditorChange}
      />
      {error && <div id="error">{error}</div>}
    </div>
  );
}

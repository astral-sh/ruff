import lzstring from "lz-string";
import Editor, { useMonaco } from "@monaco-editor/react";
import { MarkerSeverity } from "monaco-editor/esm/vs/editor/editor.api";
import { useEffect, useState, useCallback } from "react";

import init, { Check, check } from "./pkg/ruff.js";
import { AVAILABLE_OPTIONS } from "./ruff_options";
import { Config, getDefaultConfig, toRuffConfig } from "./config";
import { Options } from "./Options";

const DEFAULT_SOURCE = "print(1 + 2)";

function restoreConfigAndSource(): [Config, string] {
  let value = lzstring.decompressFromEncodedURIComponent(
    window.location.hash.slice(1)
  );
  let config = {};
  let source = DEFAULT_SOURCE;

  if (value) {
    let parts = value.split("$$$");
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

function App() {
  const monaco = useMonaco();
  const [ruffInitialized, setRuffInitialized] = useState<boolean>(false);
  const [config, setConfig] = useState<Config | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    init().then(() => setRuffInitialized(true));
  }, []);

  useEffect(() => {
    if (source === null && config === null && monaco) {
      let [config, source] = restoreConfigAndSource();
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
    let editor = monaco?.editor;
    let model = editor?.getModels()[0];
    if (
      !editor ||
      !model ||
      !ruffInitialized ||
      source === null ||
      config === null
    ) {
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
  }, [config, source, monaco, ruffInitialized]);

  const handleEditorChange = useCallback(
    (value: string | undefined) => {
      value && setSource(value);
    },
    [setSource]
  );

  const handleOptionChange = useCallback(
    (groupName: string, fieldName: string, value: string) => {
      let group = Object.assign({}, (config || {})[groupName]);
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
        onOptionChange={handleOptionChange}
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

export default App;

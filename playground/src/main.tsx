import React from "react";
import ReactDOM from "react-dom/client";
import Editor from "./Editor";
import "./index.css";
import { loader } from "@monaco-editor/react";
import { setupMonaco } from "./Editor/setupMonaco";
import { restore, stringify } from "./Editor/settings";
import { DEFAULT_PYTHON_SOURCE } from "./constants";
import init, { Workspace } from "./ruff";

startPlayground()
  .then(({ sourceCode, settings, ruffVersion }) => {
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>
        <Editor
          initialSettings={settings}
          initialSource={sourceCode}
          ruffVersion={ruffVersion}
        />
      </React.StrictMode>,
    );
  })
  .catch((error) => console.error("Failed to start playground", error));

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<{
  sourceCode: string;
  settings: string;
  ruffVersion: string;
}> {
  const initialized = init();
  loader.init().then(setupMonaco);
  await initialized;

  const response = await restore();
  const [settingsSource, pythonSource] = response ?? [
    stringify(Workspace.defaultSettings()),
    DEFAULT_PYTHON_SOURCE,
  ];

  return {
    sourceCode: pythonSource,
    settings: settingsSource,
    ruffVersion: Workspace.version(),
  };
}

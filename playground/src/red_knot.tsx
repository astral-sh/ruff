import React from "react";
import ReactDOM from "react-dom/client";
import Editor from "./red_knot/Editor";
import "./index.css";
import { loader } from "@monaco-editor/react";
import { setupMonaco } from "./shared/setupMonaco";
import { DEFAULT_PYTHON_SOURCE } from "./constants";
import init from "./red_knot/pkg";

startPlayground()
  .then(({ sourceCode }) => {
    console.log("Render", sourceCode);
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>
        <Editor initialSource={sourceCode} version="0.0.0" />
      </React.StrictMode>,
    );
  })
  .catch((error) => {
    console.error("Failed to start playground", error);
  });

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<{
  sourceCode: string;
}> {
  const initialized = init();
  loader.init().then(setupMonaco);
  await initialized;

  // try {
  //   // const response = await restore();
  //
  //   const [settingsSource, pythonSource] = response ?? [
  //     "",
  //     DEFAULT_PYTHON_SOURCE,
  //   ];
  //
  //   return {
  //     sourceCode: pythonSource,
  //     settings: settingsSource,
  //     ruffVersion: "0.0.0",
  //   };
  // } catch (error) {
  //   console.warn("Failed to restore editor state", error);

  return {
    sourceCode: DEFAULT_PYTHON_SOURCE,
  };
  //s  }
}

import React from "react";
import ReactDOM from "react-dom/client";
import Editor from "./Editor";
import "./index.css";
import { loader } from "@monaco-editor/react";
import { setupMonaco } from "./Editor/setupMonaco";

loader.init().then(setupMonaco);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Editor />
  </React.StrictMode>,
);

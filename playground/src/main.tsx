import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import Chrome from "./Editor/Chrome";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Chrome />
  </React.StrictMode>,
);

import MonacoEditor from "@monaco-editor/react";
import { AstralButton, Theme } from "shared";
import { ReadonlyFiles } from "../Playground";
import { Suspense, use, useState } from "react";
import { loadPyodide } from "pyodide";
import classNames from "classnames";

export enum SecondaryTool {
  "AST" = "AST",
  "Tokens" = "Tokens",
  "Run" = "Run",
}

export type SecondaryPanelResult =
  | null
  | { status: "ok"; content: string }
  | { status: "error"; error: string };

export interface SecondaryPanelProps {
  files: ReadonlyFiles;
  tool: SecondaryTool;
  result: SecondaryPanelResult;
  theme: Theme;
}

export default function SecondaryPanel({
  tool,
  result,
  files,
  theme,
}: SecondaryPanelProps) {
  return (
    <div className="flex flex-col h-full">
      <Content
        tool={tool}
        result={result}
        theme={theme}
        files={files}
        revision={files.revision}
      />
    </div>
  );
}

function Content({
  files,
  tool,
  result,
  theme,
  revision,
}: {
  tool: SecondaryTool;
  files: ReadonlyFiles;
  revision: number;
  result: SecondaryPanelResult;
  theme: Theme;
}) {
  if (result == null) {
    return "";
  } else {
    let language;
    switch (result.status) {
      case "ok":
        switch (tool) {
          case "AST":
            language = "RustPythonAst";
            break;

          case "Tokens":
            language = "RustPythonTokens";
            break;

          case "Run":
            return <Run theme={theme} files={files} key={`${revision}`} />;
        }

        return (
          <div className="flex-grow">
            <MonacoEditor
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 14,
                roundedSelection: false,
                scrollBeyondLastLine: false,
                contextmenu: false,
              }}
              language={language}
              value={result.content}
              theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
            />
          </div>
        );
      case "error":
        return (
          <div className="flex-grow">
            <code className="whitespace-pre-wrap text-gray-900 dark:text-gray-100">
              {result.error}
            </code>
          </div>
        );
    }
  }
}

const SANDBOX_BASE_DIRECTORY = "/playground/";

function Run({ files, theme }: { files: ReadonlyFiles; theme: Theme }) {
  const [runOutput, setRunOutput] = useState<Promise<string> | null>(null);
  const handleRun = () => {
    const output = (async () => {
      const pyodide = await loadPyodide({
        env: {
          HOME: SANDBOX_BASE_DIRECTORY,
        },
      });

      let combined_output = "";

      const outputHandler = (output: string) => {
        combined_output += output + "\n";
      };

      pyodide.setStdout({ batched: outputHandler });
      pyodide.setStderr({ batched: outputHandler });

      const main = files.selected == null ? "" : files.contents[files.selected];

      let fileName = "main.py";
      for (const file of files.index) {
        const last_separator = file.name.lastIndexOf("/");

        if (last_separator !== -1) {
          const directory =
            SANDBOX_BASE_DIRECTORY + file.name.slice(0, last_separator);
          pyodide.FS.mkdirTree(directory);
        }
        pyodide.FS.writeFile(
          SANDBOX_BASE_DIRECTORY + file.name,
          files.contents[file.id],
        );

        if (file.id === files.selected) {
          fileName = file.name;
        }
      }

      const dict = pyodide.globals.get("dict");
      const globals = dict();

      try {
        // Patch `reveal_type` to print runtime values
        pyodide.runPython(`
        import builtins

        def reveal_type(obj):
          import typing
          print(f"Runtime value is '{obj}'")
          return typing.reveal_type(obj)

        builtins.reveal_type = reveal_type`);

        pyodide.runPython(main, {
          globals,
          locals: globals,
          filename: fileName,
        });

        return combined_output;
      } catch (e) {
        return `Failed to run Python script: ${e}`;
      } finally {
        globals.destroy();
        dict.destroy();
      }
    })();
    setRunOutput(output);
  };

  if (runOutput == null) {
    return (
      <div className="flex flex-auto flex-col justify-center  items-center">
        <AstralButton
          type="button"
          className="flex-none leading-6 py-1.5 px-3 shadow-xs"
          onClick={handleRun}
        >
          <span
            className="inset-0 flex items-center justify-center"
            aria-hidden="false"
          >
            Run...
          </span>
        </AstralButton>
      </div>
    );
  }

  return (
    <Suspense
      fallback={<div className="text-center dark:text-white">Loading</div>}
    >
      <RunOutput theme={theme} runOutput={runOutput} />
    </Suspense>
  );
}

function RunOutput({
  runOutput,
  theme,
}: {
  theme: Theme;
  runOutput: Promise<string>;
}) {
  const output = use(runOutput);

  return (
    <pre
      className={classNames(
        "m-2",
        "text-sm",
        "whitespace-pre",
        theme === "dark" ? "text-white" : null,
      )}
    >
      {output}
    </pre>
  );
}

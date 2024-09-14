import { Theme } from "./theme";
import { useCallback, useEffect, useState } from "react";
import { editor } from "monaco-editor";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;
import IModel = editor.IModel;
import IModelDeltaDecoration = editor.IModelDeltaDecoration;
import MonacoEditor from "@monaco-editor/react";

export enum SecondaryTool {
  "Format" = "Format",
  "AST" = "AST",
  "Tokens" = "Tokens",
  "FIR" = "FIR",
  "Comments" = "Comments",
}

export type SecondaryPanelResult =
  | null
  | { status: "ok"; content: string }
  | { status: "error"; error: string };

export type SecondaryPanelProps = {
  tool: SecondaryTool;
  result: SecondaryPanelResult;
  theme: Theme;
  onSelectSourceByteRange(start: number, end: number): void;
};

export default function SecondaryPanel({
  tool,
  result,
  theme,
  onSelectSourceByteRange,
}: SecondaryPanelProps) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex-grow">
        <Content
          tool={tool}
          result={result}
          theme={theme}
          onSelectSourceByteRange={onSelectSourceByteRange}
        />
      </div>
    </div>
  );
}

function Content({
  tool,
  result,
  theme,
  onSelectSourceByteRange,
}: {
  tool: SecondaryTool;
  result: SecondaryPanelResult;
  theme: Theme;
  onSelectSourceByteRange(start: number, end: number): void;
}) {
  const [editor, setEditor] = useState<IStandaloneCodeEditor | null>(null);

  useEffect(() => {
    const model = editor?.getModel();
    if (editor == null || model == null) {
      return;
    }

    const handler = editor.onMouseDown((event) => {
      if (event.target.range == null) {
        return;
      }

      const byteRange = model
        .getDecorationsInRange(
          event.target.range,
          undefined,
          true,
          false,
          false,
        )
        .map((decoration) => {
          const text = model.getValueInRange(decoration.range);
          const match = text.match(/^(\d+)\.\.(\d+)$/);

          const startByteOffset = parseInt(match?.[1] ?? "", 10);
          const endByteOffset = parseInt(match?.[2] ?? "", 10);

          if (Number.isNaN(startByteOffset) || Number.isNaN(endByteOffset)) {
            return null;
          }

          return { start: startByteOffset, end: endByteOffset };
        })
        .find((range) => range != null);

      if (byteRange == null) {
        return;
      }

      onSelectSourceByteRange(byteRange.start, byteRange.end);
    });

    return () => handler.dispose();
  }, [editor, onSelectSourceByteRange]);

  const handleDidMount = useCallback((editor: IStandaloneCodeEditor) => {
    setEditor(editor);

    const model = editor?.getModel();

    if (editor == null || model == null) {
      return;
    }

    const collection = editor.createDecorationsCollection(
      createRangeDecorations(model),
    );

    const handler = model.onDidChangeContent(() => {
      collection.set(createRangeDecorations(model));
    });

    return () => handler.dispose();
  }, []);

  if (result == null) {
    return "";
  } else {
    let language;
    switch (result.status) {
      case "ok":
        switch (tool) {
          case "Format":
            language = "python";
            break;

          case "AST":
            language = "RustPythonAst";
            break;

          case "Tokens":
            language = "RustPythonTokens";
            break;

          case "FIR":
            language = "fir";
            break;

          case "Comments":
            language = "Comments";
            break;
        }

        return (
          <MonacoEditor
            options={{
              readOnly: true,
              minimap: { enabled: false },
              fontSize: 14,
              roundedSelection: false,
              scrollBeyondLastLine: false,
              contextmenu: false,
            }}
            onMount={handleDidMount}
            language={language}
            value={result.content}
            theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
          />
        );
      case "error":
        return <code className="whitespace-pre-wrap">{result.error}</code>;
    }
  }
}

function createRangeDecorations(model: IModel): Array<IModelDeltaDecoration> {
  const byteRanges = model.findMatches(
    String.raw`(\d+)\.\.(\d+)`,
    false,
    true,
    false,
    ",",
    false,
  );

  return byteRanges.map((match) => {
    return {
      range: match.range,
      options: {
        inlineClassName:
          "underline decoration-slate-600 decoration-1 cursor-pointer",
      },
    };
  });
}

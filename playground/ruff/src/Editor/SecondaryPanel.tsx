import { Theme } from "shared";
import { useCallback, useEffect, useState } from "react";
import { editor, Range } from "monaco-editor";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;
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
  selectionOffset: number | null;
  onSourceByteRangeClicked(start: number, end: number): void;
};

export default function SecondaryPanel({
  tool,
  result,
  theme,
  selectionOffset,
  onSourceByteRangeClicked,
}: SecondaryPanelProps) {
  return (
    <div className="flex flex-col h-full">
      <div className="grow">
        <Content
          tool={tool}
          result={result}
          theme={theme}
          selectionOffset={selectionOffset}
          onSourceByteRangeClicked={onSourceByteRangeClicked}
        />
      </div>
    </div>
  );
}

function Content({
  tool,
  result,
  theme,
  selectionOffset,
  onSourceByteRangeClicked,
}: {
  tool: SecondaryTool;
  result: SecondaryPanelResult;
  theme: Theme;
  selectionOffset: number | null;
  onSourceByteRangeClicked(start: number, end: number): void;
}) {
  const [editor, setEditor] = useState<IStandaloneCodeEditor | null>(null);
  const [prevSelection, setPrevSelection] = useState<number | null>(null);
  const [ranges, setRanges] = useState<
    Array<{ byteRange: { start: number; end: number }; textRange: Range }>
  >([]);

  if (
    editor != null &&
    selectionOffset != null &&
    selectionOffset !== prevSelection
  ) {
    const range = ranges.findLast(
      (range) =>
        range.byteRange.start <= selectionOffset &&
        range.byteRange.end >= selectionOffset,
    );

    if (range != null) {
      editor.revealRange(range.textRange);
      editor.setSelection(range.textRange);
    }
    setPrevSelection(selectionOffset);
  }

  useEffect(() => {
    const model = editor?.getModel();
    if (editor == null || model == null) {
      return;
    }

    const handler = editor.onMouseDown((event) => {
      if (event.target.range == null) {
        return;
      }

      const range = model
        .getDecorationsInRange(
          event.target.range,
          undefined,
          true,
          false,
          false,
        )
        .map((decoration) => {
          const decorationRange = decoration.range;
          return ranges.find((range) =>
            Range.equalsRange(range.textRange, decorationRange),
          );
        })
        .find((range) => range != null);

      if (range == null) {
        return;
      }

      onSourceByteRangeClicked(range.byteRange.start, range.byteRange.end);
    });

    return () => handler.dispose();
  }, [editor, onSourceByteRangeClicked, ranges]);

  const handleDidMount = useCallback((editor: IStandaloneCodeEditor) => {
    setEditor(editor);

    const model = editor.getModel();
    const collection = editor.createDecorationsCollection([]);

    function updateRanges() {
      if (model == null) {
        setRanges([]);
        collection.set([]);
        return;
      }

      const matches = model.findMatches(
        String.raw`(\d+)\.\.(\d+)`,
        false,
        true,
        false,
        ",",
        true,
      );

      const ranges = matches
        .map((match) => {
          const startByteOffset = parseInt(match.matches![1] ?? "", 10);
          const endByteOffset = parseInt(match.matches![2] ?? "", 10);

          if (Number.isNaN(startByteOffset) || Number.isNaN(endByteOffset)) {
            return null;
          }

          return {
            byteRange: { start: startByteOffset, end: endByteOffset },
            textRange: match.range,
          };
        })
        .filter((range) => range != null);

      setRanges(ranges);

      const decorations = ranges.map((range) => {
        return {
          range: range.textRange,
          options: {
            inlineClassName:
              "underline decoration-slate-600 decoration-1 cursor-pointer",
          },
        };
      });

      collection.set(decorations);
    }

    updateRanges();
    const handler = editor.onDidChangeModelContent(updateRanges);

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

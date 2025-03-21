import {
  useCallback,
  useDeferredValue,
  useMemo,
  useRef,
  useState,
} from "react";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Diagnostic, Workspace } from "ruff_wasm";
import {
  ErrorMessage,
  Theme,
  HorizontalResizeHandle,
  VerticalResizeHandle,
} from "shared";
import PrimarySideBar from "./PrimarySideBar";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import SecondarySideBar from "./SecondarySideBar";
import SettingsEditor from "./SettingsEditor";
import SourceEditor from "./SourceEditor";
import Diagnostics from "./Diagnostics";
import { editor } from "monaco-editor";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

type Tab = "Source" | "Settings";

export interface Source {
  pythonSource: string;
  settingsSource: string;
}

interface CheckResult {
  diagnostics: Diagnostic[];
  error: string | null;
  secondary: SecondaryPanelResult;
}

type Props = {
  source: Source;
  theme: Theme;

  onSourceChanged(source: string): void;
  onSettingsChanged(settings: string): void;
};

export default function Editor({
  source,
  theme,
  onSourceChanged,
  onSettingsChanged,
}: Props) {
  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const [tab, setTab] = useState<Tab>("Source");
  const [secondaryTool, setSecondaryTool] = useState<SecondaryTool | null>(
    () => {
      const secondaryValue = new URLSearchParams(location.search).get(
        "secondary",
      );
      if (secondaryValue == null) {
        return null;
      } else {
        return parseSecondaryTool(secondaryValue);
      }
    },
  );
  const [selection, setSelection] = useState<number | null>(null);

  // Ideally this would be retrieved right from the URL... but routing without a proper
  // router is hard (there's no location changed event) and pulling in a router
  // feels overkill.
  const handleSecondaryToolSelected = (tool: SecondaryTool | null) => {
    if (tool === secondaryTool) {
      tool = null;
    }

    const url = new URL(location.href);

    if (tool == null) {
      url.searchParams.delete("secondary");
    } else {
      url.searchParams.set("secondary", tool);
    }

    history.replaceState(null, "", url);

    setSecondaryTool(tool);
  };

  const handleGoTo = useCallback((line: number, column: number) => {
    const editor = editorRef.current;

    if (editor == null) {
      return;
    }

    const range = {
      startLineNumber: line,
      startColumn: column,
      endLineNumber: line,
      endColumn: column,
    };
    editor.revealRange(range);
    editor.setSelection(range);
  }, []);

  const handleSourceEditorMount = useCallback(
    (editor: IStandaloneCodeEditor) => {
      editorRef.current = editor;

      editor.addAction({
        contextMenuGroupId: "navigation",
        contextMenuOrder: 0,
        id: "reveal-node",
        label: "Reveal node",
        precondition: "editorTextFocus",

        run(editor: editor.ICodeEditor): void | Promise<void> {
          const position = editor.getPosition();
          if (position == null) {
            return;
          }

          const offset = editor.getModel()!.getOffsetAt(position);

          setSelection(
            charOffsetToByteOffset(editor.getModel()!.getValue(), offset),
          );
        },
      });
    },
    [],
  );

  const handleSelectByteRange = useCallback(
    (startByteOffset: number, endByteOffset: number) => {
      const model = editorRef.current?.getModel();

      if (model == null || editorRef.current == null) {
        return;
      }

      const startCharacterOffset = byteOffsetToCharOffset(
        source.pythonSource,
        startByteOffset,
      );
      const endCharacterOffset = byteOffsetToCharOffset(
        source.pythonSource,
        endByteOffset,
      );

      const start = model.getPositionAt(startCharacterOffset);
      const end = model.getPositionAt(endCharacterOffset);

      const range = {
        startLineNumber: start.lineNumber,
        startColumn: start.column,
        endLineNumber: end.lineNumber,
        endColumn: end.column,
      };
      editorRef.current?.revealRange(range);
      editorRef.current?.setSelection(range);
    },
    [source.pythonSource],
  );

  const deferredSource = useDeferredValue(source);

  const checkResult: CheckResult = useMemo(() => {
    const { pythonSource, settingsSource } = deferredSource;

    try {
      const config = JSON.parse(settingsSource);
      const workspace = new Workspace(config);
      const diagnostics = workspace.check(pythonSource);

      let secondary: SecondaryPanelResult = null;

      try {
        switch (secondaryTool) {
          case "AST":
            secondary = {
              status: "ok",
              content: workspace.parse(pythonSource),
            };
            break;

          case "Format":
            secondary = {
              status: "ok",
              content: workspace.format(pythonSource),
            };
            break;

          case "FIR":
            secondary = {
              status: "ok",
              content: workspace.format_ir(pythonSource),
            };
            break;

          case "Comments":
            secondary = {
              status: "ok",
              content: workspace.comments(pythonSource),
            };
            break;

          case "Tokens":
            secondary = {
              status: "ok",
              content: workspace.tokens(pythonSource),
            };
            break;
        }
      } catch (error: unknown) {
        secondary = {
          status: "error",
          error: error instanceof Error ? error.message : error + "",
        };
      }

      return {
        diagnostics,
        error: null,
        secondary,
      };
    } catch (e) {
      return {
        diagnostics: [],
        error: (e as Error).message,
        secondary: null,
      };
    }
  }, [deferredSource, secondaryTool]);

  return (
    <>
      <PanelGroup direction="horizontal" autoSaveId="main">
        <PrimarySideBar onSelectTool={(tool) => setTab(tool)} selected={tab} />

        <Panel id="main" order={0} minSize={10}>
          <PanelGroup id="vertical" direction="vertical">
            <Panel minSize={10} className="my-2" order={0}>
              <SourceEditor
                visible={tab === "Source"}
                source={source.pythonSource}
                theme={theme}
                diagnostics={checkResult.diagnostics}
                onChange={onSourceChanged}
                onMount={handleSourceEditorMount}
              />
              <SettingsEditor
                visible={tab === "Settings"}
                source={source.settingsSource}
                theme={theme}
                onChange={onSettingsChanged}
              />
            </Panel>
            {tab === "Source" && (
              <>
                <VerticalResizeHandle />
                <Panel
                  id="diagnostics"
                  minSize={3}
                  order={1}
                  className="my-2 flex grow"
                >
                  <Diagnostics
                    diagnostics={checkResult.diagnostics}
                    onGoTo={handleGoTo}
                    theme={theme}
                  />
                </Panel>
              </>
            )}
          </PanelGroup>
        </Panel>
        {secondaryTool != null && (
          <>
            <HorizontalResizeHandle />
            <Panel
              id="secondary-panel"
              order={1}
              className={"my-2"}
              minSize={10}
            >
              <SecondaryPanel
                theme={theme}
                tool={secondaryTool}
                result={checkResult.secondary}
                selectionOffset={selection}
                onSourceByteRangeClicked={handleSelectByteRange}
              />
            </Panel>
          </>
        )}
        <SecondarySideBar
          selected={secondaryTool}
          onSelected={handleSecondaryToolSelected}
        />
      </PanelGroup>

      {checkResult.error && tab === "Source" ? (
        <div
          style={{
            position: "fixed",
            left: "10%",
            right: "10%",
            bottom: "10%",
          }}
        >
          <ErrorMessage>{checkResult.error}</ErrorMessage>
        </div>
      ) : null}
    </>
  );
}

function parseSecondaryTool(tool: string): SecondaryTool | null {
  if (Object.hasOwn(SecondaryTool, tool)) {
    return tool as any;
  }

  return null;
}

function byteOffsetToCharOffset(content: string, byteOffset: number): number {
  // Create a Uint8Array from the UTF-8 string
  const encoder = new TextEncoder();
  const utf8Bytes = encoder.encode(content);

  // Slice the byte array up to the byteOffset
  const slicedBytes = utf8Bytes.slice(0, byteOffset);

  // Decode the sliced bytes to get a substring
  const decoder = new TextDecoder("utf-8");
  const decodedString = decoder.decode(slicedBytes);
  return decodedString.length;
}

function charOffsetToByteOffset(content: string, charOffset: number): number {
  // Create a Uint8Array from the UTF-8 string
  const encoder = new TextEncoder();
  const utf8Bytes = encoder.encode(content.substring(0, charOffset));

  return utf8Bytes.length;
}

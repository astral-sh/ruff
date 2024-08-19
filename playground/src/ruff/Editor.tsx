import { useDeferredValue, useMemo, useState } from "react";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Diagnostic, Workspace } from "./ruff_wasm";
import { ErrorMessage } from "../shared/ErrorMessage";
import PrimarySideBar from "./PrimarySideBar";
import { HorizontalResizeHandle } from "../shared/ResizeHandle";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import SecondarySideBar from "./SecondarySideBar";
import SettingsEditor from "./SettingsEditor";
import SourceEditor from "./SourceEditor";
import { Theme } from "../shared/theme";

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
        <Panel id="main" order={0} className="my-2" minSize={10}>
          <SourceEditor
            visible={tab === "Source"}
            source={source.pythonSource}
            theme={theme}
            diagnostics={checkResult.diagnostics}
            onChange={onSourceChanged}
          />
          <SettingsEditor
            visible={tab === "Settings"}
            source={source.settingsSource}
            theme={theme}
            onChange={onSettingsChanged}
          />
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

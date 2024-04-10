import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Panel, PanelGroup } from "react-resizable-panels";
import { DEFAULT_PYTHON_SOURCE } from "../constants";
import init, { Diagnostic, Workspace } from "../pkg/ruff_wasm";
import { ErrorMessage } from "./ErrorMessage";
import Header from "./Header";
import PrimarySideBar from "./PrimarySideBar";
import { HorizontalResizeHandle } from "./ResizeHandle";
import SecondaryPanel, {
  SecondaryPanelResult,
  SecondaryTool,
} from "./SecondaryPanel";
import SecondarySideBar from "./SecondarySideBar";
import { persist, persistLocal, restore, stringify } from "./settings";
import SettingsEditor from "./SettingsEditor";
import SourceEditor from "./SourceEditor";
import { useTheme } from "./theme";

type Tab = "Source" | "Settings";

interface Source {
  pythonSource: string;
  settingsSource: string;
  revision: number;
}

interface CheckResult {
  diagnostics: Diagnostic[];
  error: string | null;
  secondary: SecondaryPanelResult;
}

export default function Editor() {
  const [ruffVersion, setRuffVersion] = useState<string | null>(null);
  const [checkResult, setCheckResult] = useState<CheckResult>({
    diagnostics: [],
    error: null,
    secondary: null,
  });
  const [source, setSource] = useState<Source | null>(null);

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
  const [theme, setTheme] = useTheme();

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

  useEffect(() => {
    async function initAsync() {
      await init();
      const response = await restore();
      const [settingsSource, pythonSource] = response ?? [
        stringify(Workspace.defaultSettings()),
        DEFAULT_PYTHON_SOURCE,
      ];

      setSource({
        revision: 0,
        pythonSource,
        settingsSource,
      });
      setRuffVersion(Workspace.version());
    }

    initAsync().catch(console.error);
  }, []);

  const deferredSource = useDeferredValue(source);

  useEffect(() => {
    if (deferredSource == null) {
      return;
    }

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

      setCheckResult({
        diagnostics,
        error: null,
        secondary,
      });
    } catch (e) {
      setCheckResult({
        diagnostics: [],
        error: (e as Error).message,
        secondary: null,
      });
    }
  }, [deferredSource, secondaryTool]);

  useEffect(() => {
    if (source != null) {
      persistLocal(source);
    }
  }, [source]);

  const handleShare = useMemo(() => {
    if (source == null) {
      return undefined;
    }

    return () => {
      return persist(source.settingsSource, source.pythonSource);
    };
  }, [source]);

  const handlePythonSourceChange = useCallback((pythonSource: string) => {
    setSource((state) =>
      state
        ? {
            ...state,
            pythonSource,
            revision: state.revision + 1,
          }
        : null,
    );
  }, []);

  const handleSettingsSourceChange = useCallback((settingsSource: string) => {
    setSource((state) =>
      state
        ? {
            ...state,
            settingsSource,
            revision: state.revision + 1,
          }
        : null,
    );
  }, []);

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        edit={source ? source.revision : null}
        theme={theme}
        version={ruffVersion}
        onChangeTheme={setTheme}
        onShare={handleShare}
      />

      <div className="flex flex-grow">
        {source ? (
          <PanelGroup direction="horizontal" autoSaveId="main">
            <PrimarySideBar
              onSelectTool={(tool) => setTab(tool)}
              selected={tab}
            />
            <Panel id="main" order={0} className="my-2" minSize={10}>
              <SourceEditor
                visible={tab === "Source"}
                source={source.pythonSource}
                theme={theme}
                diagnostics={checkResult.diagnostics}
                onChange={handlePythonSourceChange}
              />
              <SettingsEditor
                visible={tab === "Settings"}
                source={source.settingsSource}
                theme={theme}
                onChange={handleSettingsSourceChange}
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
        ) : null}
      </div>
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
    </main>
  );
}

function parseSecondaryTool(tool: string): SecondaryTool | null {
  if (Object.hasOwn(SecondaryTool, tool)) {
    return tool as any;
  }

  return null;
}

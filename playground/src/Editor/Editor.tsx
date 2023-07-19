import { useCallback, useEffect, useMemo, useState } from "react";
import { DEFAULT_PYTHON_SOURCE } from "../constants";
import init, {
  check,
  Diagnostic,
  currentVersion,
  defaultSettings,
} from "../pkg";
import { ErrorMessage } from "./ErrorMessage";
import Header from "./Header";
import { useTheme } from "./theme";
import { persist, restore, stringify } from "./settings";
import SettingsEditor from "./SettingsEditor";
import SourceEditor from "./SourceEditor";
import MonacoThemes from "./MonacoThemes";

type Tab = "Source" | "Settings";

interface Source {
  pythonSource: string;
  settingsSource: string;
  revision: number;
}

interface CheckResult {
  diagnostics: Diagnostic[];
  error: string | null;
}

export default function Editor() {
  const [ruffVersion, setRuffVersion] = useState<string | null>(null);
  const [checkResult, setCheckResult] = useState<CheckResult>({
    diagnostics: [],
    error: null,
  });
  const [source, setSource] = useState<Source>({
    pythonSource: "",
    settingsSource: "",
    revision: 0,
  });

  const [tab, setTab] = useState<Tab>("Source");
  const [theme, setTheme] = useTheme();

  const initialized = ruffVersion != null;

  useEffect(() => {
    init().then(() => {
      setRuffVersion(currentVersion());

      const [settingsSource, pythonSource] = restore() ?? [
        stringify(defaultSettings()),
        DEFAULT_PYTHON_SOURCE,
      ];

      setSource({
        pythonSource,
        revision: 0,
        settingsSource,
      });
    });
  }, []);

  useEffect(() => {
    if (!initialized) {
      return;
    }

    const { settingsSource, pythonSource } = source;

    try {
      const config = JSON.parse(settingsSource);
      const diagnostics = check(pythonSource, config);

      setCheckResult({
        diagnostics,
        error: null,
      });
    } catch (e) {
      setCheckResult({
        diagnostics: [],
        error: (e as Error).message,
      });
    }
  }, [initialized, source]);

  const handleShare = useMemo(() => {
    if (!initialized) {
      return undefined;
    }

    return () => {
      persist(source.settingsSource, source.pythonSource);
    };
  }, [source, initialized]);

  const handlePythonSourceChange = useCallback((pythonSource: string) => {
    setSource((state) => ({
      ...state,
      pythonSource,
      revision: state.revision + 1,
    }));
  }, []);

  const handleSettingsSourceChange = useCallback((settingsSource: string) => {
    setSource((state) => ({
      ...state,
      settingsSource,
      revision: state.revision + 1,
    }));
  }, []);

  return (
    <main
      className={
        "h-full w-full flex flex-auto bg-ayu-background dark:bg-ayu-background-dark"
      }
    >
      <Header
        edit={source.revision}
        tab={tab}
        theme={theme}
        version={ruffVersion}
        onChangeTab={setTab}
        onChangeTheme={setTheme}
        onShare={handleShare}
      />

      <MonacoThemes />

      <div className={"mt-12 relative flex-auto"}>
        {initialized ? (
          <>
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
          </>
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

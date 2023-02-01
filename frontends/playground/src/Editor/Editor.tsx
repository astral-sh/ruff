import { useCallback, useEffect, useState } from "react";
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

export default function Editor() {
  const [initialized, setInitialized] = useState<boolean>(false);
  const [version, setVersion] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("Source");
  const [edit, setEdit] = useState<number>(0);
  const [settingsSource, setSettingsSource] = useState<string | null>(null);
  const [pythonSource, setPythonSource] = useState<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [theme, setTheme] = useTheme();

  useEffect(() => {
    init().then(() => setInitialized(true));
  }, []);

  useEffect(() => {
    if (!initialized || settingsSource == null || pythonSource == null) {
      return;
    }

    let config: any;
    let diagnostics: Diagnostic[];

    try {
      config = JSON.parse(settingsSource);
    } catch (e) {
      setDiagnostics([]);
      setError((e as Error).message);
      return;
    }

    try {
      diagnostics = check(pythonSource, config);
    } catch (e) {
      setError(e as string);
      return;
    }

    setError(null);
    setDiagnostics(diagnostics);
  }, [initialized, settingsSource, pythonSource]);

  useEffect(() => {
    if (!initialized) {
      return;
    }

    if (settingsSource == null || pythonSource == null) {
      const payload = restore();
      if (payload) {
        const [settingsSource, pythonSource] = payload;
        setSettingsSource(settingsSource);
        setPythonSource(pythonSource);
      } else {
        setSettingsSource(stringify(defaultSettings()));
        setPythonSource(DEFAULT_PYTHON_SOURCE);
      }
    }
  }, [initialized, settingsSource, pythonSource]);

  useEffect(() => {
    if (!initialized) {
      return;
    }

    setVersion(currentVersion());
  }, [initialized]);

  const handleShare = useCallback(() => {
    if (!initialized || settingsSource == null || pythonSource == null) {
      return;
    }

    persist(settingsSource, pythonSource);
  }, [initialized, settingsSource, pythonSource]);

  const handlePythonSourceChange = useCallback((pythonSource: string) => {
    setEdit((edit) => edit + 1);
    setPythonSource(pythonSource);
  }, []);

  const handleSettingsSourceChange = useCallback((settingsSource: string) => {
    setEdit((edit) => edit + 1);
    setSettingsSource(settingsSource);
  }, []);

  return (
    <main
      className={
        "h-full w-full flex flex-auto bg-ayu-background dark:bg-ayu-background-dark"
      }
    >
      <Header
        edit={edit}
        tab={tab}
        theme={theme}
        version={version}
        onChangeTab={setTab}
        onChangeTheme={setTheme}
        onShare={initialized ? handleShare : undefined}
      />

      <MonacoThemes />

      <div className={"mt-12 relative flex-auto"}>
        {initialized && settingsSource != null && pythonSource != null ? (
          <>
            <SourceEditor
              visible={tab === "Source"}
              source={pythonSource}
              theme={theme}
              diagnostics={diagnostics}
              onChange={handlePythonSourceChange}
            />
            <SettingsEditor
              visible={tab === "Settings"}
              source={settingsSource}
              theme={theme}
              onChange={handleSettingsSourceChange}
            />
          </>
        ) : null}
      </div>
      {error && tab === "Source" ? (
        <div
          style={{
            position: "fixed",
            left: "10%",
            right: "10%",
            bottom: "10%",
          }}
        >
          <ErrorMessage>{error}</ErrorMessage>
        </div>
      ) : null}
    </main>
  );
}

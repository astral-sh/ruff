import { useCallback, useEffect, useState } from "react";
import { persist, restore } from "./config";
import { DEFAULT_CONFIG_SOURCE, DEFAULT_PYTHON_SOURCE } from "../constants";
import { ErrorMessage } from "./ErrorMessage";
import Header from "./Header";
import init, { check, current_version, Check } from "../pkg";
import SettingsEditor from "./SettingsEditor";
import SourceEditor from "./SourceEditor";
import Themes from "./Themes";

type Tab = "Source" | "Settings";

export default function Editor() {
  const [initialized, setInitialized] = useState<boolean>(false);
  const [version, setVersion] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("Source");
  const [edit, setEdit] = useState<number>(0);
  const [configSource, setConfigSource] = useState<string | null>(null);
  const [pythonSource, setPythonSource] = useState<string | null>(null);
  const [checks, setChecks] = useState<Check[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    init().then(() => setInitialized(true));
  }, []);

  useEffect(() => {
    if (!initialized || configSource == null || pythonSource == null) {
      return;
    }

    let config: any;
    let checks: Check[];

    try {
      config = JSON.parse(configSource);
    } catch (e) {
      setChecks([]);
      setError((e as Error).message);
      return;
    }

    try {
      checks = check(pythonSource, config);
    } catch (e) {
      setError(e as string);
      return;
    }

    setError(null);
    setChecks(checks);
  }, [initialized, configSource, pythonSource]);

  useEffect(() => {
    if (configSource == null || pythonSource == null) {
      const payload = restore();
      if (payload) {
        const [configSource, pythonSource] = payload;
        setConfigSource(configSource);
        setPythonSource(pythonSource);
      } else {
        setConfigSource(DEFAULT_CONFIG_SOURCE);
        setPythonSource(DEFAULT_PYTHON_SOURCE);
      }
    }
  }, [configSource, pythonSource]);

  useEffect(() => {
    if (!initialized) {
      return;
    }

    setVersion(current_version());
  }, [initialized]);

  const handleShare = useCallback(() => {
    if (!initialized || configSource == null || pythonSource == null) {
      return;
    }

    persist(configSource, pythonSource);
  }, [initialized, configSource, pythonSource]);

  const handlePythonSourceChange = useCallback((pythonSource: string) => {
    setEdit((edit) => edit + 1);
    setPythonSource(pythonSource);
  }, []);

  const handleConfigSourceChange = useCallback((configSource: string) => {
    setEdit((edit) => edit + 1);
    setConfigSource(configSource);
  }, []);

  return (
    <main className={"h-full w-full flex flex-auto"}>
      <Header
        edit={edit}
        version={version}
        tab={tab}
        onChange={setTab}
        onShare={initialized ? handleShare : undefined}
      />

      <Themes />

      <div className={"mt-12 relative flex-auto"}>
        {initialized && configSource != null && pythonSource != null ? (
          <>
            <SourceEditor
              visible={tab === "Source"}
              source={pythonSource}
              checks={checks}
              onChange={handlePythonSourceChange}
            />
            <SettingsEditor
              visible={tab === "Settings"}
              source={configSource}
              onChange={handleConfigSourceChange}
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

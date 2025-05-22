import ruffSchema from "../../../../ruff.schema.json";
import { useCallback, useMemo, useRef, useState } from "react";
import { Header, useTheme, setupMonaco } from "shared";
import { persist, persistLocal, restore, stringify } from "./settings";
import { default as Editor, Source } from "./Editor";
import initRuff, { Workspace } from "ruff_wasm";
import { loader } from "@monaco-editor/react";
import { DEFAULT_PYTHON_SOURCE } from "../constants";

export default function Chrome() {
  const initPromise = useRef<null | Promise<void>>(null);
  const [pythonSource, setPythonSource] = useState<null | string>(null);
  const [settings, setSettings] = useState<null | string>(null);
  const [revision, setRevision] = useState(0);
  const [ruffVersion, setRuffVersion] = useState<string | null>(null);

  const [theme, setTheme] = useTheme();

  const handleShare = useCallback(() => {
    if (settings == null || pythonSource == null) {
      return;
    }

    persist(settings, pythonSource).catch((error) =>
      // eslint-disable-next-line no-console
      console.error(`Failed to share playground: ${error}`),
    );
  }, [pythonSource, settings]);

  if (initPromise.current == null) {
    initPromise.current = startPlayground()
      .then(({ sourceCode, settings, ruffVersion }) => {
        setPythonSource(sourceCode);
        setSettings(settings);
        setRuffVersion(ruffVersion);
        setRevision(1);
      })
      .catch((error) => {
        // eslint-disable-next-line no-console
        console.error("Failed to initialize playground.", error);
      });
  }

  const handleSourceChanged = useCallback(
    (source: string) => {
      setPythonSource(source);
      setRevision((revision) => revision + 1);

      if (settings != null) {
        persistLocal({ pythonSource: source, settingsSource: settings });
      }
    },
    [settings],
  );

  const handleSettingsChanged = useCallback(
    (settings: string) => {
      setSettings(settings);
      setRevision((revision) => revision + 1);

      if (pythonSource != null) {
        persistLocal({ pythonSource: pythonSource, settingsSource: settings });
      }
    },
    [pythonSource],
  );

  const handleResetClicked = useCallback(() => {
    const pythonSource = DEFAULT_PYTHON_SOURCE;
    const settings = stringify(Workspace.defaultSettings());

    persistLocal({ pythonSource, settingsSource: settings });
    setPythonSource(pythonSource);
    setSettings(settings);
  }, []);

  const source: Source | null = useMemo(() => {
    if (pythonSource == null || settings == null) {
      return null;
    }

    return { pythonSource, settingsSource: settings };
  }, [settings, pythonSource]);

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        edit={revision}
        theme={theme}
        tool="ruff"
        version={ruffVersion}
        onChangeTheme={setTheme}
        onShare={handleShare}
        onReset={handleResetClicked}
      />

      <div className="flex grow">
        {source != null && (
          <Editor
            theme={theme}
            source={source}
            onSettingsChanged={handleSettingsChanged}
            onSourceChanged={handleSourceChanged}
          />
        )}
      </div>
    </main>
  );
}

// Run once during startup. Initializes monaco, loads the wasm file, and restores the previous editor state.
async function startPlayground(): Promise<{
  sourceCode: string;
  settings: string;
  ruffVersion: string;
}> {
  await initRuff();
  const monaco = await loader.init();

  setupMonaco(monaco, {
    uri: "https://raw.githubusercontent.com/astral-sh/ruff/main/ruff.schema.json",
    fileMatch: ["ruff.json"],
    schema: ruffSchema,
  });

  const response = await restore();
  const [settingsSource, pythonSource] = response ?? [
    stringify(Workspace.defaultSettings()),
    DEFAULT_PYTHON_SOURCE,
  ];

  return {
    sourceCode: pythonSource,
    settings: settingsSource,
    ruffVersion: Workspace.version(),
  };
}

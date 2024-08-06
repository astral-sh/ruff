import { useCallback, useDeferredValue, useMemo, useState } from "react";
import { Panel, PanelGroup } from "react-resizable-panels";
import { Settings, TargetVersion, Workspace, FileHandle } from "./pkg";
import { ErrorMessage } from "../shared/ErrorMessage";
import Header from "../shared/Header";
import PrimarySideBar from "./PrimarySideBar";
import { HorizontalResizeHandle } from "../shared/ResizeHandle";

import { useTheme } from "../shared/theme";
import { Files } from "./Files";
import SourceEditor from "../ruff/SourceEditor";

type Tab = "Source" | "Settings";

interface CheckResult {
  diagnostics: string[];
  error: string | null;
}

type CurrentFile = {
  handle: FileHandle;
  name: string;
  content: string;
};

type FileIndex = {
  [name: string]: FileHandle | null;
};

type Props = {
  initialSource: string;
  version: string;
};

export default function Editor({ initialSource }: Props) {
  const [workspace, setWorkspace] = useState(() => {
    const settings = new Settings(TargetVersion.Py312);
    return new Workspace("/", settings);
  });

  const [files, setFiles] = useState<FileIndex>({});

  // The revision gets incremented everytime any persisted state changes.
  const [revision, setRevision] = useState(0);

  const [currentFile, setCurrentFile] = useState<CurrentFile>(() => {
    const handle = workspace.openFile("main.py", initialSource);
    return {
      handle,
      content: initialSource,
      name: "main.py",
    };
  });

  const [tab, setTab] = useState<Tab>("Source");
  // const [secondaryTool, setSecondaryTool] = useState<SecondaryTool | null>(
  //   () => {
  //     const secondaryValue = new URLSearchParams(location.search).get(
  //       "secondary",
  //     );
  //     if (secondaryValue == null) {
  //       return null;
  //     } else {
  //       return parseSecondaryTool(secondaryValue);
  //     }
  //   },
  // );

  const [theme, setTheme] = useTheme();

  // Ideally this would be retrieved right from the URL... but routing without a proper
  // router is hard (there's no location changed event) and pulling in a router
  // feels overkill.
  // const handleSecondaryToolSelected = (tool: SecondaryTool | null) => {
  //   if (tool === secondaryTool) {
  //     tool = null;
  //   }
  //
  //   const url = new URL(location.href);
  //
  //   if (tool == null) {
  //     url.searchParams.delete("secondary");
  //   } else {
  //     url.searchParams.set("secondary", tool);
  //   }
  //
  //   history.replaceState(null, "", url);
  //
  //   setSecondaryTool(tool);
  // };

  // TODO: figure out how to do deferred
  const deferredSource = useDeferredValue(currentFile);

  const checkResult: CheckResult = useMemo(() => {
    const file = deferredSource;

    try {
      const diagnostics = workspace.checkFile(file.handle);

      // let secondary: SecondaryPanelResult = null;

      // try {
      //   switch (secondaryTool) {
      //     case "AST":
      //       secondary = {
      //         status: "ok",
      //         content: workspace.parsed(file),
      //       };
      //       break;
      //
      //     case "Format":
      //       secondary = {
      //         status: "error",
      //         content: "Not supported",
      //       };
      //       break;
      //
      //     case "FIR":
      //       secondary = {
      //         status: "error",
      //         content: "Not supported",
      //       };
      //       break;
      //
      //     case "Comments":
      //       secondary = {
      //         status: "error",
      //         content: "Not supported",
      //       };
      //       break;
      //
      //     case "Tokens":
      //       secondary = {
      //         status: "ok",
      //         content: workspace.tokens(file),
      //       };
      //       break;
      //   }
      // } catch (error: unknown) {
      //   secondary = {
      //     status: "error",
      //     error: error instanceof Error ? error.message : error + "",
      //   };
      // }

      return {
        diagnostics,
        error: null,
        // secondary,
      };
    } catch (e) {
      return {
        diagnostics: [],
        error: (e as Error).message,
        // secondary: null,
      };
    }
  }, [deferredSource]);

  const handleShare = useCallback(() => {
    console.log("TODO");
    // persist(source.settingsSource, source.pythonSource).catch((error) =>
    //   console.error(`Failed to share playground: ${error}`),
    // );
  }, []);

  const handlePythonSourceChange = useCallback((pythonSource: string) => {
    workspace.updateFile(currentFile.handle, pythonSource);
    setCurrentFile({
      ...currentFile,
      content: pythonSource,
    });
    setRevision((revision) => revision + 1);
  }, []);

  // const handleSettingsSourceChange = useCallback((settingsSource: string) => {
  //   setSource((source) => {
  //     const newSource = {
  //       ...source,
  //       settingsSource,
  //       revision: source.revision + 1,
  //     };
  //
  //     persistLocal(newSource);
  //     return newSource;
  //   });
  // }, []);

  const handleFileAdded = useCallback(
    (name: string) => {
      const handle = workspace.openFile(name, "");

      setFiles({
        ...files,
        [name]: handle,
      });

      setRevision((revision) => revision + 1);

      setCurrentFile({
        handle,
        name: name,
        content: "",
      });

      return handle;
    },
    [workspace, files],
  );

  const handleFileRemoved = useCallback(
    (name: string) => {
      const file = files[name];

      if (file != null) {
        workspace.closeFile(file);

        const newFiles = { ...files };
        delete newFiles[name];

        setFiles(newFiles);
        setRevision((revision) => (revision += 1));

        if (currentFile.handle == file) {
          handleFileClicked(Object.keys(files)[0]);
        }
      }
    },
    [files, workspace],
  );

  const handleFileRenamed = useCallback(
    (oldName: string, newName: string) => {
      const oldFile = files[oldName];

      if (oldFile == null) {
        return;
      }

      const content = workspace.sourceText(oldFile);
      handleFileRemoved(oldName);
      const newFile = handleFileAdded(newName);
      workspace.updateFile(newFile, content);
    },
    [files, workspace],
  );

  const handleFileClicked = useCallback(
    (name: string) => {
      const file = files[name]!;

      setCurrentFile({
        handle: file,
        name,
        content: workspace.sourceText(file),
      });
    },
    [files, workspace],
  );

  const fileNames = useMemo(() => Object.keys(files), [files]);

  return (
    <main className="flex flex-col h-full bg-ayu-background dark:bg-ayu-background-dark">
      <Header
        edit={revision}
        theme={theme}
        version={version}
        onChangeTheme={setTheme}
        onShare={handleShare}
      />

      <div className="flex flex-grow">
        {
          <PanelGroup direction="horizontal" autoSaveId="main">
            <Files
              files={fileNames}
              onAdd={handleFileAdded}
              onRename={handleFileRenamed}
              onSelected={handleFileClicked}
              onRemove={handleFileRemoved}
              selected={tab}
            />
            <Panel id="main" order={0} className="my-2" minSize={10}>
              <SourceEditor
                key={currentFile.handle}
                visible={tab === "Source"}
                source={currentFile.content}
                theme={theme}
                diagnostics={checkResult.diagnostics}
                onChange={handlePythonSourceChange}
              />
              {/*<SettingsEditor*/}
              {/*  visible={tab === "Settings"}*/}
              {/*  source={source.settingsSource}*/}
              {/*  theme={theme}*/}
              {/*  onChange={handleSettingsSourceChange}*/}
              {/*/>*/}
            </Panel>
            {/*{secondaryTool != null && (*/}
            {/*  <>*/}
            {/*    <HorizontalResizeHandle />*/}
            {/*    <Panel*/}
            {/*      id="secondary-panel"*/}
            {/*      order={1}*/}
            {/*      className={"my-2"}*/}
            {/*      minSize={10}*/}
            {/*    >*/}
            {/*      /!*<SecondaryPanel*!/*/}
            {/*      /!*  theme={theme}*!/*/}
            {/*      /!*  tool={secondaryTool}*!/*/}
            {/*      /!*  result={checkResult.secondary}*!/*/}
            {/*      /!>*!/*/}
            {/*    </Panel>*/}
            {/*  </>*/}
            {/*)}*/}
            {/*<SecondarySideBar*/}
            {/*  selected={secondaryTool}*/}
            {/*  onSelected={handleSecondaryToolSelected}*/}
            {/*/>*/}
          </PanelGroup>
        }
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

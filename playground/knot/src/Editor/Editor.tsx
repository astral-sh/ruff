/**
 * Editor for the Python source code.
 */

import Moncao, { Monaco, OnMount } from "@monaco-editor/react";
import {
  CancellationToken,
  editor,
  IDisposable,
  IPosition,
  IRange,
  languages,
  MarkerSeverity,
  Position,
  Uri,
} from "monaco-editor";
import { RefObject, useCallback, useEffect, useRef } from "react";
import { Theme } from "shared";
import {
  Diagnostic,
  Severity,
  Workspace,
  Position as KnotPosition,
  type Range as KnotRange,
} from "red_knot_wasm";

import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;
import { FileId, ReadonlyFiles } from "../Playground";
import { isPythonFile } from "./Files";

type Props = {
  visible: boolean;
  fileName: string;
  selected: FileId;
  files: ReadonlyFiles;
  diagnostics: Diagnostic[];
  theme: Theme;
  workspace: Workspace;
  onChange(content: string): void;
  onMount(editor: IStandaloneCodeEditor, monaco: Monaco): void;
  onOpenFile(file: FileId): void;
};

export default function Editor({
  visible,
  fileName,
  selected,
  files,
  theme,
  diagnostics,
  workspace,
  onChange,
  onMount,
  onOpenFile,
}: Props) {
  const disposable = useRef<{
    typeDefinition: IDisposable;
    editorOpener: IDisposable;
  } | null>(null);
  const playgroundState = useRef<PlaygroundServerProps>({
    monaco: null,
    files,
    workspace,
    onOpenFile,
  });

  playgroundState.current = {
    monaco: playgroundState.current.monaco,
    files,
    workspace,
    onOpenFile,
  };

  // Update the diagnostics in the editor.
  useEffect(() => {
    const monaco = playgroundState.current.monaco;

    if (monaco == null) {
      return;
    }

    updateMarkers(monaco, workspace, diagnostics);
  }, [workspace, diagnostics]);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  useEffect(() => {
    return () => {
      disposable.current?.typeDefinition.dispose();
      disposable.current?.editorOpener.dispose();
    };
  }, []);

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      updateMarkers(instance, workspace, diagnostics);

      const server = new PlaygroundServer(playgroundState);
      const typeDefinitionDisposable =
        instance.languages.registerTypeDefinitionProvider("python", server);
      const editorOpenerDisposable =
        instance.editor.registerEditorOpener(server);

      disposable.current = {
        typeDefinition: typeDefinitionDisposable,
        editorOpener: editorOpenerDisposable,
      };

      playgroundState.current.monaco = instance;

      onMount(editor, instance);
    },

    [onMount, workspace, diagnostics],
  );

  return (
    <Moncao
      onMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: true,
      }}
      language={fileName.endsWith(".pyi") ? "python" : undefined}
      path={fileName}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={files.contents[selected]}
      onChange={handleChange}
    />
  );
}

function updateMarkers(
  monaco: Monaco,
  workspace: Workspace,
  diagnostics: Array<Diagnostic>,
) {
  const editor = monaco.editor;
  const model = editor?.getModels()[0];

  if (!model) {
    return;
  }

  editor.setModelMarkers(
    model,
    "owner",
    diagnostics.map((diagnostic) => {
      const mapSeverity = (severity: Severity) => {
        switch (severity) {
          case Severity.Info:
            return MarkerSeverity.Info;
          case Severity.Warning:
            return MarkerSeverity.Warning;
          case Severity.Error:
            return MarkerSeverity.Error;
          case Severity.Fatal:
            return MarkerSeverity.Error;
        }
      };

      const range = diagnostic.toRange(workspace);

      return {
        code: diagnostic.id(),
        startLineNumber: range?.start?.line ?? 0,
        startColumn: range?.start?.column ?? 0,
        endLineNumber: range?.end?.line ?? 0,
        endColumn: range?.end?.column ?? 0,
        message: diagnostic.message(),
        severity: mapSeverity(diagnostic.severity()),
        tags: [],
      };
    }),
  );
}

interface PlaygroundServerProps {
  monaco: Monaco | null;
  workspace: Workspace;
  files: ReadonlyFiles;

  onOpenFile: (file: FileId) => void;
}

class PlaygroundServer
  implements languages.TypeDefinitionProvider, editor.ICodeEditorOpener
{
  constructor(private props: RefObject<PlaygroundServerProps>) {}

  provideTypeDefinition(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _: CancellationToken,
  ): languages.ProviderResult<languages.Definition | languages.LocationLink[]> {
    const workspace = this.props.current.workspace;

    const selectedFile = this.props.current.files.selected;
    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.current.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const links = workspace.gotoTypeDefinition(
      selectedHandle,
      new KnotPosition(position.lineNumber, position.column),
    );

    const locations = links.map((link) => {
      const targetSelection =
        link.selection_range == null
          ? undefined
          : knotRangeToIRange(link.selection_range);

      const originSelection =
        link.origin_selection_range == null
          ? undefined
          : knotRangeToIRange(link.origin_selection_range);

      return {
        uri: Uri.parse(link.path),
        range: knotRangeToIRange(link.full_range),
        targetSelectionRange: targetSelection,
        originSelectionRange: originSelection,
      } as languages.LocationLink;
    });

    return locations;
  }

  openCodeEditor(
    source: editor.ICodeEditor,
    resource: Uri,
    selectionOrPosition?: IRange | IPosition,
  ): boolean {
    const files = this.props.current.files;
    const monaco = this.props.current.monaco;

    if (monaco == null) {
      return false;
    }

    const fileId = files.index.find((file) => {
      return Uri.file(file.name).toString() === resource.toString();
    })?.id;

    if (fileId == null) {
      return false;
    }

    const handle = files.handles[fileId];

    let model = monaco.editor.getModel(resource);
    if (model == null) {
      const language =
        handle != null && isPythonFile(handle) ? "python" : undefined;
      model = monaco.editor.createModel(
        files.contents[fileId],
        language,
        resource,
      );
    }

    // it's a bit hacky to create the model manually
    // but only using `onOpenFile` isn't enough
    // because the model doesn't get updated until the next render.
    if (files.selected !== fileId) {
      source.setModel(model);

      this.props.current.onOpenFile(fileId);
    }

    if (selectionOrPosition != null) {
      if (Position.isIPosition(selectionOrPosition)) {
        source.setPosition(selectionOrPosition);
        source.revealPosition(selectionOrPosition);
      } else {
        source.setSelection(selectionOrPosition);
        source.revealPosition({
          lineNumber: selectionOrPosition.startLineNumber,
          column: selectionOrPosition.startColumn,
        });
      }
    }

    return true;
  }
}

function knotRangeToIRange(range: KnotRange): IRange {
  return {
    startLineNumber: range.start.line,
    startColumn: range.start.column,
    endLineNumber: range.end.line,
    endColumn: range.end.column,
  };
}

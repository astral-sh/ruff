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
  Range,
  Uri,
} from "monaco-editor";
import { RefObject, useCallback, useEffect, useRef } from "react";
import { Theme } from "shared";
import {
  Severity,
  type Workspace,
  Position as KnotPosition,
  type Range as KnotRange,
} from "red_knot_wasm";

import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;
import { FileId, ReadonlyFiles } from "../Playground";
import { isPythonFile } from "./Files";
import { Diagnostic } from "./Diagnostics";

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
  onFileOpened(file: FileId): void;
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
  onFileOpened,
}: Props) {
  const disposable = useRef<{
    typeDefinition: IDisposable;
    editorOpener: IDisposable;
    hover: IDisposable;
    inlayHints: IDisposable;
  } | null>(null);
  const playgroundState = useRef<PlaygroundServerProps>({
    monaco: null,
    files,
    workspace,
    onFileOpened,
  });

  playgroundState.current = {
    monaco: playgroundState.current.monaco,
    files,
    workspace,
    onFileOpened,
  };

  // Update the diagnostics in the editor.
  useEffect(() => {
    const monaco = playgroundState.current.monaco;

    if (monaco == null) {
      return;
    }

    updateMarkers(monaco, diagnostics);
  }, [diagnostics]);

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
      disposable.current?.hover.dispose();
      disposable.current?.inlayHints.dispose();
    };
  }, []);

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      updateMarkers(instance, diagnostics);

      const server = new PlaygroundServer(playgroundState);
      const typeDefinitionDisposable =
        instance.languages.registerTypeDefinitionProvider("python", server);
      const hoverDisposable = instance.languages.registerHoverProvider(
        "python",
        server,
      );
      const inlayHintsDisposable =
        instance.languages.registerInlayHintsProvider("python", server);
      const editorOpenerDisposable =
        instance.editor.registerEditorOpener(server);

      disposable.current = {
        typeDefinition: typeDefinitionDisposable,
        editorOpener: editorOpenerDisposable,
        hover: hoverDisposable,
        inlayHints: inlayHintsDisposable,
      };

      playgroundState.current.monaco = instance;

      onMount(editor, instance);
    },

    [onMount, diagnostics],
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

function updateMarkers(monaco: Monaco, diagnostics: Array<Diagnostic>) {
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

      const range = diagnostic.range;

      return {
        code: diagnostic.id,
        startLineNumber: range?.start?.line ?? 0,
        startColumn: range?.start?.column ?? 0,
        endLineNumber: range?.end?.line ?? 0,
        endColumn: range?.end?.column ?? 0,
        message: diagnostic.message,
        severity: mapSeverity(diagnostic.severity),
        tags: [],
      };
    }),
  );
}

interface PlaygroundServerProps {
  monaco: Monaco | null;
  workspace: Workspace;
  files: ReadonlyFiles;

  onFileOpened: (file: FileId) => void;
}

class PlaygroundServer
  implements
    languages.TypeDefinitionProvider,
    editor.ICodeEditorOpener,
    languages.HoverProvider,
    languages.InlayHintsProvider
{
  constructor(private props: RefObject<PlaygroundServerProps>) {}

  provideInlayHints(
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _model: editor.ITextModel,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _range: Range,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.InlayHintList> {
    const workspace = this.props.current.workspace;
    const selectedFile = this.props.current.files.selected;

    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.current.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const inlayHints = workspace.inlayHints(selectedHandle);

    if (inlayHints.length === 0) {
      return undefined;
    }

    return {
      dispose: () => {},
      hints: inlayHints.map(
        (hint: {
          position: { line: number; column: number };
          markdown: string;
        }) => {
          return {
            label: hint.markdown,
            position: {
              lineNumber: hint.position.line,
              column: hint.position.column,
            },
          };
        },
      ),
    };
  }

  resolveInlayHint(
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _hint: languages.InlayHint,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.InlayHint> {
    return undefined;
  }

  provideHover(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context?: languages.HoverContext<languages.Hover> | undefined,
  ): languages.ProviderResult<languages.Hover> {
    const workspace = this.props.current.workspace;

    const selectedFile = this.props.current.files.selected;
    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.current.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const hover = workspace.hover(
      selectedHandle,
      new KnotPosition(position.lineNumber, position.column),
    );

    if (hover == null) {
      return;
    }

    return {
      range: knotRangeToIRange(hover.range),
      contents: [{ value: hover.markdown, isTrusted: true }],
    };
  }

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

    return (
      links
        .map((link) => {
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
        })
        // Filter out vendored files because they aren't open in the editor.
        .filter((link) => link.uri.scheme !== "vendored")
    );
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

      this.props.current.onFileOpened(fileId);
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

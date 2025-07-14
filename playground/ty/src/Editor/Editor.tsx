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
import { useCallback, useEffect, useRef } from "react";
import { Theme } from "shared";
import {
  Position as TyPosition,
  Range as TyRange,
  SemanticToken,
  Severity,
  type Workspace,
  CompletionKind,
} from "ty_wasm";
import { FileId, ReadonlyFiles } from "../Playground";
import { isPythonFile } from "./Files";
import { Diagnostic } from "./Diagnostics";
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;
import CompletionItemKind = languages.CompletionItemKind;

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
  const serverRef = useRef<PlaygroundServer | null>(null);

  if (serverRef.current != null) {
    serverRef.current.update({
      files,
      workspace,
      onOpenFile,
    });
  }

  // Update the diagnostics in the editor.
  useEffect(() => {
    const server = serverRef.current;

    if (server == null) {
      return;
    }

    server.updateDiagnostics(diagnostics);
  }, [diagnostics]);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  useEffect(() => {
    return () => {
      const server = serverRef.current;

      if (server != null) {
        server.dispose();
      }
    };
  }, []);

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      serverRef.current?.dispose();

      const server = new PlaygroundServer(instance, {
        workspace,
        files,
        onOpenFile,
      });

      server.updateDiagnostics(diagnostics);
      serverRef.current = server;

      onMount(editor, instance);
    },

    [files, onOpenFile, workspace, onMount, diagnostics],
  );

  return (
    <Moncao
      key={files.playgroundRevision}
      onMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: false,
        minimap: { enabled: false },
        fontSize: 14,
        roundedSelection: false,
        scrollBeyondLastLine: false,
        contextmenu: true,
        "semanticHighlighting.enabled": true,
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

interface PlaygroundServerProps {
  workspace: Workspace;
  files: ReadonlyFiles;
  onOpenFile: (file: FileId) => void;
}

class PlaygroundServer
  implements
    languages.TypeDefinitionProvider,
    editor.ICodeEditorOpener,
    languages.HoverProvider,
    languages.InlayHintsProvider,
    languages.DocumentFormattingEditProvider,
    languages.CompletionItemProvider,
    languages.SignatureHelpProvider,
    languages.DocumentSemanticTokensProvider,
    languages.DocumentRangeSemanticTokensProvider
{
  private typeDefinitionProviderDisposable: IDisposable;
  private editorOpenerDisposable: IDisposable;
  private hoverDisposable: IDisposable;
  private inlayHintsDisposable: IDisposable;
  private formatDisposable: IDisposable;
  private completionDisposable: IDisposable;
  private signatureHelpDisposable: IDisposable;
  private semanticTokensDisposable: IDisposable;
  private rangeSemanticTokensDisposable: IDisposable;

  constructor(
    private monaco: Monaco,
    private props: PlaygroundServerProps,
  ) {
    this.typeDefinitionProviderDisposable =
      monaco.languages.registerTypeDefinitionProvider("python", this);
    this.hoverDisposable = monaco.languages.registerHoverProvider(
      "python",
      this,
    );
    this.inlayHintsDisposable = monaco.languages.registerInlayHintsProvider(
      "python",
      this,
    );
    this.completionDisposable = monaco.languages.registerCompletionItemProvider(
      "python",
      this,
    );
    this.signatureHelpDisposable = monaco.languages.registerSignatureHelpProvider(
      "python",
      this,
    );
    this.semanticTokensDisposable =
      monaco.languages.registerDocumentSemanticTokensProvider("python", this);
    this.rangeSemanticTokensDisposable =
      monaco.languages.registerDocumentRangeSemanticTokensProvider(
        "python",
        this,
      );
    this.editorOpenerDisposable = monaco.editor.registerEditorOpener(this);
    this.formatDisposable =
      monaco.languages.registerDocumentFormattingEditProvider("python", this);
  }

  triggerCharacters: string[] = ["."];
  signatureHelpTriggerCharacters: string[] = ["(", ","];

  getLegend(): languages.SemanticTokensLegend {
    return {
      tokenTypes: SemanticToken.kinds(),
      tokenModifiers: SemanticToken.modifiers(),
    };
  }

  provideDocumentSemanticTokens(
    model: editor.ITextModel,
  ): languages.SemanticTokens | null {
    const selectedFile = this.props.files.selected;

    if (selectedFile == null) {
      return null;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return null;
    }

    const tokens = this.props.workspace.semanticTokens(selectedHandle);
    return generateMonacoTokens(tokens, model);
  }

  releaseDocumentSemanticTokens() {}

  provideDocumentRangeSemanticTokens(
    model: editor.ITextModel,
    range: Range,
  ): languages.SemanticTokens | null {
    const selectedFile = this.props.files.selected;

    if (selectedFile == null) {
      return null;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return null;
    }

    const tyRange = monacoRangeToTyRange(range);

    const tokens = this.props.workspace.semanticTokensInRange(
      selectedHandle,
      tyRange,
    );

    return generateMonacoTokens(tokens, model);
  }

  provideCompletionItems(
    model: editor.ITextModel,
    position: Position,
  ): languages.ProviderResult<languages.CompletionList> {
    const selectedFile = this.props.files.selected;

    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const completions = this.props.workspace.completions(
      selectedHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    // If completions is 100, this gives us "99" which has a length of two
    const digitsLength = String(completions.length - 1).length;

    return {
      suggestions: completions.map((completion, i) => ({
        label: completion.name,
        sortText: String(i).padStart(digitsLength, "0"),
        kind:
          completion.kind == null
            ? CompletionItemKind.Variable
            : mapCompletionKind(completion.kind),
        insertText: completion.name,
        // TODO(micha): It's unclear why this field is required for monaco but not VS Code.
        //  and omitting it works just fine? The LSP doesn't expose this information right now
        //  which is why we go with undefined for now.
        range: undefined as any,
      })),
    };
  }

  resolveCompletionItem: undefined;

  provideSignatureHelp(
    model: editor.ITextModel,
    position: Position,
    _token: CancellationToken,
    _context: languages.SignatureHelpContext,
  ): languages.ProviderResult<languages.SignatureHelp> {
    const selectedFile = this.props.files.selected;
    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];
    if (selectedHandle == null) {
      return;
    }

    const info = this.props.workspace.signatureHelp(
      selectedHandle,
      new TyPosition(position.lineNumber, position.column),
    );
    if (info == null) {
      return;
    }

    debugger;

    return {
      value: {
        signatures: info.signatures.map((sig) => ({
          label: sig.label,
          documentation: sig.documentation,
          parameters: sig.parameters.map((param) => ({
            label: param.label,
            documentation: param.documentation,
          })),
        })),
        activeSignature: info.active_signature,
        activeParameter: info.active_parameter,
      },
      dispose: () => {},
    };
  }

  provideInlayHints(
    _model: editor.ITextModel,
    range: Range,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.InlayHintList> {
    const workspace = this.props.workspace;
    const selectedFile = this.props.files.selected;

    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const inlayHints = workspace.inlayHints(
      selectedHandle,
      monacoRangeToTyRange(range),
    );

    if (inlayHints.length === 0) {
      return undefined;
    }

    return {
      dispose: () => {},
      hints: inlayHints.map((hint) => ({
        label: hint.markdown,
        position: {
          lineNumber: hint.position.line,
          column: hint.position.column,
        },
      })),
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

  update(props: PlaygroundServerProps) {
    this.props = props;
  }

  updateDiagnostics(diagnostics: Array<Diagnostic>) {
    if (this.props.files.selected == null) {
      return;
    }

    const handle = this.props.files.handles[this.props.files.selected];

    if (handle == null) {
      return;
    }

    const editor = this.monaco.editor;
    const model = editor.getModel(Uri.parse(handle.path()));

    if (model == null) {
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

  provideHover(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context?: languages.HoverContext<languages.Hover> | undefined,
  ): languages.ProviderResult<languages.Hover> {
    const workspace = this.props.workspace;

    const selectedFile = this.props.files.selected;
    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const hover = workspace.hover(
      selectedHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    if (hover == null) {
      return;
    }

    return {
      range: tyRangeToMonacoRange(hover.range),
      contents: [{ value: hover.markdown, isTrusted: true }],
    };
  }

  provideTypeDefinition(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _: CancellationToken,
  ): languages.ProviderResult<languages.Definition | languages.LocationLink[]> {
    const workspace = this.props.workspace;

    const selectedFile = this.props.files.selected;
    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const links = workspace.gotoTypeDefinition(
      selectedHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return (
      links
        .map((link) => {
          const targetSelection =
            link.selection_range == null
              ? undefined
              : tyRangeToMonacoRange(link.selection_range);

          const originSelection =
            link.origin_selection_range == null
              ? undefined
              : tyRangeToMonacoRange(link.origin_selection_range);

          return {
            uri: Uri.parse(link.path),
            range: tyRangeToMonacoRange(link.full_range),
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
    const files = this.props.files;

    const fileId = files.index.find((file) => {
      return Uri.file(file.name).toString() === resource.toString();
    })?.id;

    if (fileId == null) {
      return false;
    }

    const handle = files.handles[fileId];

    let model = this.monaco.editor.getModel(resource);
    if (model == null) {
      const language =
        handle != null && isPythonFile(handle) ? "python" : undefined;
      model = this.monaco.editor.createModel(
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

      this.props.onOpenFile(fileId);
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

  provideDocumentFormattingEdits(
    model: editor.ITextModel,
  ): languages.ProviderResult<languages.TextEdit[]> {
    if (this.props.files.selected == null) {
      return null;
    }

    const fileHandle = this.props.files.handles[this.props.files.selected];

    if (fileHandle == null) {
      return null;
    }

    const formatted = this.props.workspace.format(fileHandle);
    if (formatted != null) {
      return [
        {
          range: model.getFullModelRange(),
          text: formatted,
        },
      ];
    }

    return null;
  }

  dispose() {
    this.hoverDisposable.dispose();
    this.editorOpenerDisposable.dispose();
    this.typeDefinitionProviderDisposable.dispose();
    this.inlayHintsDisposable.dispose();
    this.formatDisposable.dispose();
    this.rangeSemanticTokensDisposable.dispose();
    this.semanticTokensDisposable.dispose();
    this.completionDisposable.dispose();
    this.signatureHelpDisposable.dispose();
  }
}

function tyRangeToMonacoRange(range: TyRange): IRange {
  return {
    startLineNumber: range.start.line,
    startColumn: range.start.column,
    endLineNumber: range.end.line,
    endColumn: range.end.column,
  };
}

function monacoRangeToTyRange(range: IRange): TyRange {
  return new TyRange(
    new TyPosition(range.startLineNumber, range.startColumn),
    new TyPosition(range.endLineNumber, range.endColumn),
  );
}

function generateMonacoTokens(
  semantic: SemanticToken[],
  model: editor.ITextModel,
): languages.SemanticTokens {
  const result = [];

  let prevLine = 0;
  let prevChar = 0;

  for (const token of semantic) {
    // Convert from 1-based to 0-based indexing for Monaco
    const line = token.range.start.line - 1;
    const char = token.range.start.column - 1;

    const length = model.getValueLengthInRange(
      tyRangeToMonacoRange(token.range),
    );

    result.push(
      line - prevLine,
      prevLine === line ? char - prevChar : char,
      length,
      token.kind,
      token.modifiers,
    );

    prevLine = line;
    prevChar = char;
  }

  return { data: Uint32Array.from(result) };
}

function mapCompletionKind(kind: CompletionKind): CompletionItemKind {
  switch (kind) {
    case CompletionKind.Text:
      return CompletionItemKind.Text;
    case CompletionKind.Method:
      return CompletionItemKind.Method;
    case CompletionKind.Function:
      return CompletionItemKind.Function;
    case CompletionKind.Constructor:
      return CompletionItemKind.Constructor;
    case CompletionKind.Field:
      return CompletionItemKind.Field;
    case CompletionKind.Variable:
      return CompletionItemKind.Variable;
    case CompletionKind.Class:
      return CompletionItemKind.Class;
    case CompletionKind.Interface:
      return CompletionItemKind.Interface;
    case CompletionKind.Module:
      return CompletionItemKind.Module;
    case CompletionKind.Property:
      return CompletionItemKind.Property;
    case CompletionKind.Unit:
      return CompletionItemKind.Unit;
    case CompletionKind.Value:
      return CompletionItemKind.Value;
    case CompletionKind.Enum:
      return CompletionItemKind.Enum;
    case CompletionKind.Keyword:
      return CompletionItemKind.Keyword;
    case CompletionKind.Snippet:
      return CompletionItemKind.Snippet;
    case CompletionKind.Color:
      return CompletionItemKind.Color;
    case CompletionKind.File:
      return CompletionItemKind.File;
    case CompletionKind.Reference:
      return CompletionItemKind.Reference;
    case CompletionKind.Folder:
      return CompletionItemKind.Folder;
    case CompletionKind.EnumMember:
      return CompletionItemKind.EnumMember;
    case CompletionKind.Constant:
      return CompletionItemKind.Constant;
    case CompletionKind.Struct:
      return CompletionItemKind.Struct;
    case CompletionKind.Event:
      return CompletionItemKind.Event;
    case CompletionKind.Operator:
      return CompletionItemKind.Operator;
    case CompletionKind.TypeParameter:
      return CompletionItemKind.TypeParameter;
  }
}

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
  type FileHandle,
  DocumentHighlight,
  DocumentHighlightKind,
  InlayHintKind,
  LocationLink,
  TextEdit,
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
  onVendoredFileChange: (vendoredFileHandle: FileHandle) => void;
  onBackToUserFile: () => void;
  isViewingVendoredFile: boolean;
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
  onVendoredFileChange,
  onBackToUserFile,
  isViewingVendoredFile = false,
}: Props) {
  const serverRef = useRef<PlaygroundServer | null>(null);

  if (serverRef.current != null) {
    serverRef.current.update(
      {
        files,
        workspace,
        onOpenFile,
        onVendoredFileChange,
        onBackToUserFile,
      },
      isViewingVendoredFile,
    );
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
      // Don't update file content when viewing vendored files
      if (!isViewingVendoredFile) {
        onChange(value ?? "");
      }
    },
    [onChange, isViewingVendoredFile],
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

      const server = new PlaygroundServer(instance, editor, {
        workspace,
        files,
        onOpenFile,
        onVendoredFileChange,
        onBackToUserFile,
      });

      server.updateDiagnostics(diagnostics);
      serverRef.current = server;

      onMount(editor, instance);
    },

    [
      files,
      onOpenFile,
      workspace,
      onMount,
      diagnostics,
      onVendoredFileChange,
      onBackToUserFile,
    ],
  );

  return (
    <Moncao
      key={files.playgroundRevision}
      onMount={handleMount}
      options={{
        fixedOverflowWidgets: true,
        readOnly: isViewingVendoredFile, // Make editor read-only for vendored files
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
  onVendoredFileChange: (vendoredFileHandle: FileHandle) => void;
  onBackToUserFile: () => void;
}

class PlaygroundServer
  implements
    languages.TypeDefinitionProvider,
    languages.DeclarationProvider,
    languages.DefinitionProvider,
    languages.ReferenceProvider,
    editor.ICodeEditorOpener,
    languages.HoverProvider,
    languages.InlayHintsProvider,
    languages.DocumentFormattingEditProvider,
    languages.CompletionItemProvider,
    languages.DocumentSemanticTokensProvider,
    languages.DocumentRangeSemanticTokensProvider,
    languages.SignatureHelpProvider,
    languages.DocumentHighlightProvider,
    languages.CodeActionProvider
{
  private diagnostics: Diagnostic[] = [];

  private typeDefinitionProviderDisposable: IDisposable;
  private declarationProviderDisposable: IDisposable;
  private definitionProviderDisposable: IDisposable;
  private referenceProviderDisposable: IDisposable;
  private editorOpenerDisposable: IDisposable;
  private hoverDisposable: IDisposable;
  private inlayHintsDisposable: IDisposable;
  private formatDisposable: IDisposable;
  private completionDisposable: IDisposable;
  private semanticTokensDisposable: IDisposable;
  private rangeSemanticTokensDisposable: IDisposable;
  private signatureHelpDisposable: IDisposable;
  private documentHighlightDisposable: IDisposable;
  private codeActionDisposable: IDisposable;
  private inVendoredFileCondition: editor.IContextKey<boolean>;
  // Cache for vendored file handles
  private vendoredFileHandles = new Map<string, FileHandle>();

  private getVendoredPath(uri: Uri): string {
    // Monaco parses "vendored://stdlib/typing.pyi" as authority="stdlib", path="/typing.pyi"
    // We need to reconstruct the full path
    return uri.authority ? `${uri.authority}${uri.path}` : uri.path;
  }

  constructor(
    private monaco: Monaco,
    private editor: IStandaloneCodeEditor,
    private props: PlaygroundServerProps,
  ) {
    this.typeDefinitionProviderDisposable =
      monaco.languages.registerTypeDefinitionProvider("python", this);
    this.declarationProviderDisposable =
      monaco.languages.registerDeclarationProvider("python", this);
    this.definitionProviderDisposable =
      monaco.languages.registerDefinitionProvider("python", this);
    this.referenceProviderDisposable =
      monaco.languages.registerReferenceProvider("python", this);
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
    this.signatureHelpDisposable =
      monaco.languages.registerSignatureHelpProvider("python", this);
    this.documentHighlightDisposable =
      monaco.languages.registerDocumentHighlightProvider("python", this);
    this.codeActionDisposable = monaco.languages.registerCodeActionProvider(
      "python",
      this,
    );

    this.inVendoredFileCondition = editor.createContextKey<boolean>(
      "inVendoredFile",
      false,
    );
    // Register Esc key command
    editor.addCommand(
      monaco.KeyCode.Escape,
      () => this.props.onBackToUserFile(),
      "inVendoredFile",
    );
  }

  triggerCharacters: string[] = ["."];
  signatureHelpTriggerCharacters: string[] = ["(", ","];
  signatureHelpRetriggerCharacters: string[] = [")"];

  getLegend(): languages.SemanticTokensLegend {
    return {
      tokenTypes: SemanticToken.kinds(),
      tokenModifiers: SemanticToken.modifiers(),
    };
  }

  provideDocumentSemanticTokens(
    model: editor.ITextModel,
  ): languages.SemanticTokens | null {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return null;
    }

    const tokens = this.props.workspace.semanticTokens(fileHandle);
    return generateMonacoTokens(tokens, model);
  }

  releaseDocumentSemanticTokens() {}

  provideDocumentRangeSemanticTokens(
    model: editor.ITextModel,
    range: Range,
  ): languages.SemanticTokens | null {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return null;
    }

    const tyRange = monacoRangeToTyRange(range);
    const tokens = this.props.workspace.semanticTokensInRange(
      fileHandle,
      tyRange,
    );
    return generateMonacoTokens(tokens, model);
  }

  provideCompletionItems(
    model: editor.ITextModel,
    position: Position,
  ): languages.ProviderResult<languages.CompletionList> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const completions = this.props.workspace.completions(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    // If completions is 100, this gives us "99" which has a length of two
    const digitsLength = String(completions.length - 1).length;

    return {
      suggestions: completions.map((completion, i) => ({
        label: {
          label: completion.name,
          detail:
            completion.module_name == null
              ? undefined
              : ` (import ${completion.module_name})`,
          description: completion.detail ?? undefined,
        },
        sortText: String(i).padStart(digitsLength, "0"),
        kind:
          completion.kind == null
            ? CompletionItemKind.Variable
            : mapCompletionKind(completion.kind),
        insertText: completion.insert_text ?? completion.name,
        additionalTextEdits: completion.additional_text_edits?.map(
          (edit: TextEdit) => ({
            range: tyRangeToMonacoRange(edit.range),
            text: edit.new_text,
          }),
        ),
        documentation: completion.documentation,
        detail: completion.detail,
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
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _context: languages.SignatureHelpContext,
  ): languages.ProviderResult<languages.SignatureHelpResult> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const signatureHelp = this.props.workspace.signatureHelp(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    if (signatureHelp == null) {
      return undefined;
    }

    return this.formatSignatureHelp(signatureHelp);
  }

  provideDocumentHighlights(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.DocumentHighlight[]> {
    const workspace = this.props.workspace;
    const selectedFile = this.props.files.selected;

    if (selectedFile == null) {
      return;
    }

    const selectedHandle = this.props.files.handles[selectedFile];

    if (selectedHandle == null) {
      return;
    }

    const highlights = workspace.documentHighlights(
      selectedHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return highlights.map((highlight: DocumentHighlight) => ({
      range: tyRangeToMonacoRange(highlight.range),
      kind: mapDocumentHighlightKind(highlight.kind),
    }));
  }

  provideInlayHints(
    model: editor.ITextModel,
    range: Range,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.InlayHintList> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const inlayHints = this.props.workspace.inlayHints(
      fileHandle,
      monacoRangeToTyRange(range),
    );

    if (inlayHints.length === 0) {
      return undefined;
    }

    function mapInlayHintKind(kind: InlayHintKind): languages.InlayHintKind {
      switch (kind) {
        case InlayHintKind.Type:
          return languages.InlayHintKind.Type;
        case InlayHintKind.Parameter:
          return languages.InlayHintKind.Parameter;
      }
    }

    return {
      dispose: () => {},
      hints: inlayHints.map((hint) => ({
        label: hint.label.map((part) => ({
          label: part.label,
          // As of 2025-09-23, location isn't supported by Monaco which is why we don't set it
        })),
        position: {
          lineNumber: hint.position.line,
          column: hint.position.column,
        },
        kind: mapInlayHintKind(hint.kind),
        textEdits: hint.text_edits.map((edit: TextEdit) => ({
          range: tyRangeToMonacoRange(edit.range),
          text: edit.new_text,
        })),
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

  update(props: PlaygroundServerProps, isViewingVendoredFile: boolean) {
    this.props = props;
    this.inVendoredFileCondition.set(isViewingVendoredFile);
  }

  private getOrCreateVendoredFileHandle(vendoredPath: string): FileHandle {
    const cachedHandle = this.vendoredFileHandles.get(vendoredPath);
    // Check if we already have a handle for this vendored file
    if (cachedHandle != null) {
      return cachedHandle;
    }

    // Use the new WASM method to get a proper file handle for the vendored file
    const handle = this.props.workspace.getVendoredFile(vendoredPath);
    this.vendoredFileHandles.set(vendoredPath, handle);
    return handle;
  }

  private getFileHandleForModel(model: editor.ITextModel) {
    // Handle vendored files
    if (model.uri.scheme === "vendored") {
      const vendoredPath = this.getVendoredPath(model.uri);

      // If not cached, try to create it
      return this.getOrCreateVendoredFileHandle(vendoredPath);
    }

    // Handle regular user files
    const selectedFile = this.props.files.selected;
    if (selectedFile == null) {
      return null;
    }

    return this.props.files.handles[selectedFile];
  }

  private formatSignatureHelp(
    signatureHelp: any,
  ): languages.SignatureHelpResult {
    return {
      dispose() {},
      value: {
        signatures: signatureHelp.signatures.map((sig: any) => ({
          label: sig.label,
          documentation: sig.documentation
            ? { value: sig.documentation }
            : undefined,
          parameters: sig.parameters.map((param: any) => ({
            label: param.label,
            documentation: param.documentation
              ? { value: param.documentation }
              : undefined,
          })),
          activeParameter: sig.active_parameter,
        })),
        activeSignature: signatureHelp.active_signature ?? 0,
        activeParameter:
          signatureHelp.active_signature != null
            ? (signatureHelp.signatures[signatureHelp.active_signature]
                ?.active_parameter ?? 0)
            : 0,
      },
    };
  }

  updateDiagnostics(diagnostics: Array<Diagnostic>) {
    this.diagnostics = diagnostics;

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

  provideCodeActions(
    model: editor.ITextModel,
    range: Range,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _context: languages.CodeActionContext,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
  ): languages.ProviderResult<languages.CodeActionList> {
    const actions: languages.CodeAction[] = [];
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    for (const diagnostic of this.diagnostics) {
      const diagnosticRange = diagnostic.range;
      if (diagnosticRange == null) {
        continue;
      }

      const monacoRange = tyRangeToMonacoRange(diagnosticRange);
      if (!Range.areIntersecting(range, monacoRange)) {
        continue;
      }

      const codeActions = this.props.workspace.codeActions(
        fileHandle,
        diagnostic.raw,
      );
      if (codeActions == null) {
        continue;
      }

      for (const codeAction of codeActions) {
        actions.push({
          title: codeAction.title,
          kind: "quickfix",
          isPreferred: codeAction.preferred,
          edit: {
            edits: codeAction.edits.map((edit) => ({
              resource: model.uri,
              textEdit: {
                range: tyRangeToMonacoRange(edit.range),
                text: edit.new_text,
              },
              versionId: model.getVersionId(),
            })),
          },
        });
      }
    }

    if (actions.length === 0) {
      return undefined;
    }

    return {
      actions,
      dispose: () => {},
    };
  }

  provideHover(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _token: CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context?: languages.HoverContext<languages.Hover> | undefined,
  ): languages.ProviderResult<languages.Hover> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const hover = this.props.workspace.hover(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    if (hover == null) {
      return undefined;
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
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const links = this.props.workspace.gotoTypeDefinition(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return this.mapNavigationTargets(links);
  }

  provideDeclaration(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _: CancellationToken,
  ): languages.ProviderResult<languages.Definition | languages.LocationLink[]> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const links = this.props.workspace.gotoDeclaration(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return this.mapNavigationTargets(links);
  }

  provideDefinition(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _: CancellationToken,
  ): languages.ProviderResult<languages.Definition | languages.LocationLink[]> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const links = this.props.workspace.gotoDefinition(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return this.mapNavigationTargets(links);
  }

  provideReferences(
    model: editor.ITextModel,
    position: Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context: languages.ReferenceContext,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    _: CancellationToken,
  ): languages.ProviderResult<languages.Location[]> {
    const fileHandle = this.getFileHandleForModel(model);
    if (fileHandle == null) {
      return undefined;
    }

    const links = this.props.workspace.gotoReferences(
      fileHandle,
      new TyPosition(position.lineNumber, position.column),
    );

    return this.mapNavigationTargets(links);
  }

  openCodeEditor(
    source: editor.ICodeEditor,
    resource: Uri,
    selectionOrPosition?: IRange | IPosition,
  ): boolean {
    const files = this.props.files;

    // Model should already exist from mapNavigationTargets for both vendored and regular files
    const model = this.monaco.editor.getModel(resource);
    if (model == null) {
      // Model should have been created by mapNavigationTargets
      return false;
    }

    // Handle file-specific logic
    if (resource.scheme === "vendored") {
      // Get the file handle to track that we're viewing a vendored file
      const vendoredPath = this.getVendoredPath(resource);
      const fileHandle = this.getOrCreateVendoredFileHandle(vendoredPath);
      this.props.onVendoredFileChange(fileHandle);
    } else {
      // Handle regular files
      const fileId = files.index.find((file) => {
        return Uri.file(file.name).toString() === resource.toString();
      })?.id;

      if (fileId == null) {
        return false;
      }

      // Set the model and trigger UI updates
      if (files.selected !== fileId) {
        this.props.onOpenFile(fileId);
      }
    }

    source.setModel(model);

    if (selectionOrPosition != null) {
      if (Position.isIPosition(selectionOrPosition)) {
        source.setPosition(selectionOrPosition);
        source.revealPositionInCenterIfOutsideViewport(selectionOrPosition);
      } else {
        source.setSelection(selectionOrPosition);
        source.revealRangeNearTopIfOutsideViewport(selectionOrPosition);
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

  private mapNavigationTarget(link: LocationLink): languages.LocationLink {
    const uri = Uri.parse(link.path);

    // Pre-create models to ensure peek definition works
    if (this.monaco.editor.getModel(uri) == null) {
      if (uri.scheme === "vendored") {
        // Handle vendored files
        const vendoredPath = this.getVendoredPath(uri);
        const fileHandle = this.getOrCreateVendoredFileHandle(vendoredPath);
        const content = this.props.workspace.sourceText(fileHandle);
        this.monaco.editor.createModel(content, "python", uri);
      } else {
        // Handle regular files
        const fileId = this.props.files.index.find((file) => {
          return Uri.file(file.name).toString() === uri.toString();
        })?.id;

        if (fileId != null) {
          const handle = this.props.files.handles[fileId];
          if (handle != null) {
            const language = isPythonFile(handle) ? "python" : undefined;
            this.monaco.editor.createModel(
              this.props.files.contents[fileId],
              language,
              uri,
            );
          }
        }
      }
    }

    const targetSelection =
      link.selection_range == null
        ? undefined
        : tyRangeToMonacoRange(link.selection_range);

    const originSelection =
      link.origin_selection_range == null
        ? undefined
        : tyRangeToMonacoRange(link.origin_selection_range);

    return {
      uri: uri,
      range: tyRangeToMonacoRange(link.full_range),
      targetSelectionRange: targetSelection,
      originSelectionRange: originSelection,
    } as languages.LocationLink;
  }

  private mapNavigationTargets(
    links: LocationLink[],
  ): languages.LocationLink[] {
    return links.map((link) => this.mapNavigationTarget(link));
  }

  dispose() {
    this.hoverDisposable.dispose();
    this.editorOpenerDisposable.dispose();
    this.typeDefinitionProviderDisposable.dispose();
    this.declarationProviderDisposable.dispose();
    this.definitionProviderDisposable.dispose();
    this.referenceProviderDisposable.dispose();
    this.inlayHintsDisposable.dispose();
    this.formatDisposable.dispose();
    this.rangeSemanticTokensDisposable.dispose();
    this.semanticTokensDisposable.dispose();
    this.completionDisposable.dispose();
    this.signatureHelpDisposable.dispose();
    this.documentHighlightDisposable.dispose();
    this.codeActionDisposable.dispose();
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

function mapDocumentHighlightKind(
  kind: DocumentHighlightKind,
): languages.DocumentHighlightKind {
  switch (kind) {
    case DocumentHighlightKind.Text:
      return languages.DocumentHighlightKind.Text;
    case DocumentHighlightKind.Read:
      return languages.DocumentHighlightKind.Read;
    case DocumentHighlightKind.Write:
      return languages.DocumentHighlightKind.Write;
    default:
      return languages.DocumentHighlightKind.Text;
  }
}

import * as monaco from "monaco-editor";
import {
  FileHandle,
  PositionEncoding,
  Workspace,
  Range as TyRange,
  Severity,
  Position as TyPosition,
  CompletionKind,
  LocationLink,
  TextEdit,
  InlayHintKind,
  Diagnostic as TyDiagnostic,
} from "ty_wasm";

// Ayu theme colors from the ty playground
const RADIATE = "#d7ff64";
const ROCK = "#78876e";
const COSMIC = "#de5fe9";
const SUN = "#ffac2f";
const ELECTRON = "#46ebe1";
const CONSTELLATION = "#5f6de9";
const STARLIGHT = "#f4f4f1";
const PROTON = "#f6afbc";
const SUPERNOVA = "#f1aff6";
const ASTEROID = "#e3cee3";

let themesInitialized = false;

function defineAyuThemes() {
  if (themesInitialized) return;
  themesInitialized = true;

  // Ayu Light theme
  monaco.editor.defineTheme("Ayu-Light", {
    inherit: false,
    base: "vs",
    colors: {
      "editor.background": "#f8f9fa",
      "editor.foreground": "#5c6166",
      "editorLineNumber.foreground": "#8a919966",
      "editorLineNumber.activeForeground": "#8a9199cc",
      "editorCursor.foreground": "#ffaa33",
      "editor.selectionBackground": "#035bd626",
      "editor.lineHighlightBackground": "#8a91991a",
      "editorIndentGuide.background": "#8a91992e",
      "editorIndentGuide.activeBackground": "#8a919959",
      "editorError.foreground": "#e65050",
      "editorWarning.foreground": "#ffaa33",
      "editorWidget.background": "#f3f4f5",
      "editorWidget.border": "#6b7d8f1f",
      "editorHoverWidget.background": "#f3f4f5",
      "editorHoverWidget.border": "#6b7d8f1f",
      "editorSuggestWidget.background": "#f3f4f5",
      "editorSuggestWidget.border": "#6b7d8f1f",
      "editorSuggestWidget.highlightForeground": "#ffaa33",
      "editorSuggestWidget.selectedBackground": "#56728f1f",
    },
    rules: [
      { fontStyle: "italic", foreground: "#787b8099", token: "comment" },
      { foreground: COSMIC, token: "keyword" },
      { foreground: COSMIC, token: "builtinConstant" },
      { foreground: CONSTELLATION, token: "number" },
      { foreground: ROCK, token: "tag" },
      { foreground: ROCK, token: "string" },
      { foreground: SUN, token: "method" },
      { foreground: SUN, token: "function" },
      { foreground: SUN, token: "decorator" },
    ],
    encodedTokensColors: [],
  });

  // Ayu Dark theme
  monaco.editor.defineTheme("Ayu-Dark", {
    inherit: false,
    base: "vs-dark",
    colors: {
      "editor.background": "#0b0e14",
      "editor.foreground": "#bfbdb6",
      "editorLineNumber.foreground": "#6c738099",
      "editorLineNumber.activeForeground": "#6c7380e6",
      "editorCursor.foreground": "#e6b450",
      "editor.selectionBackground": "#409fff4d",
      "editor.lineHighlightBackground": "#131721",
      "editorIndentGuide.background": "#6c738033",
      "editorIndentGuide.activeBackground": "#6c738080",
      "editorError.foreground": "#d95757",
      "editorWarning.foreground": "#e6b450",
      "editorWidget.background": "#0f131a",
      "editorWidget.border": "#11151c",
      "editorHoverWidget.background": "#0f131a",
      "editorHoverWidget.border": "#11151c",
      "editorSuggestWidget.background": "#0f131a",
      "editorSuggestWidget.border": "#11151c",
      "editorSuggestWidget.highlightForeground": "#e6b450",
      "editorSuggestWidget.selectedBackground": "#47526640",
    },
    rules: [
      { fontStyle: "italic", foreground: "#acb6bf8c", token: "comment" },
      { foreground: ELECTRON, token: "string" },
      { foreground: CONSTELLATION, token: "number" },
      { foreground: STARLIGHT, token: "identifier" },
      { foreground: RADIATE, token: "keyword" },
      { foreground: RADIATE, token: "builtinConstant" },
      { foreground: PROTON, token: "tag" },
      { foreground: ASTEROID, token: "delimiter" },
      { foreground: SUPERNOVA, token: "class" },
      { foreground: STARLIGHT, token: "variable" },
      { foreground: STARLIGHT, token: "parameter" },
      { foreground: SUN, token: "method" },
      { foreground: SUN, token: "function" },
      { foreground: SUN, token: "decorator" },
    ],
    encodedTokensColors: [],
  });
}

export interface EditorOptions {
  initialCode?: string;
  theme?: "light" | "dark";
  fileName?: string;
  settings?: Record<string, any>;
  height?: string;
  showDiagnostics?: boolean;
  id?: string;
}

interface Diagnostic {
  id: string;
  message: string;
  severity: Severity;
  range: TyRange | null;
  raw: TyDiagnostic;
}

const DEFAULT_SETTINGS = {
  environment: {
    "python-version": "3.14",
  },
  rules: {
    "undefined-reveal": "ignore",
  },
};

export class EmbeddableEditor {
  private container: HTMLElement;
  private options: Required<EditorOptions>;
  private editor: monaco.editor.IStandaloneCodeEditor | null = null;
  private workspace: Workspace | null = null;
  private fileHandle: FileHandle | null = null;
  private languageServer: LanguageServer | null = null;
  private diagnosticsContainer: HTMLElement | null = null;
  private editorContainer: HTMLElement | null = null;
  private errorContainer: HTMLElement | null = null;
  private checkTimeoutId: number | null = null;

  constructor(container: HTMLElement | string, options: EditorOptions) {
    const element =
      typeof container === "string"
        ? document.querySelector(container)
        : container;

    if (!element) {
      throw new Error(`Container not found: ${container}`);
    }

    this.container = element as HTMLElement;
    this.options = {
      initialCode: options.initialCode ?? "",
      theme: options.theme ?? "light",
      fileName: options.fileName ?? "main.py",
      settings: options.settings ?? DEFAULT_SETTINGS,
      height: options.height ?? "400px",
      showDiagnostics: options.showDiagnostics ?? true,
      id: options.id ?? `editor-${Date.now()}`,
    };

    this.init();
  }

  private async init() {
    try {
      // Create container structure
      this.createContainerStructure();

      // Initialize ty workspace
      const ty = await import("ty_wasm");
      await ty.default();

      this.workspace = new Workspace("/", PositionEncoding.Utf16, {});
      this.workspace.updateOptions(this.options.settings);
      this.fileHandle = this.workspace.openFile(
        this.options.fileName,
        this.options.initialCode,
      );

      // Initialize Monaco editor
      if (this.editorContainer) {
        // Define Ayu themes before creating the editor
        defineAyuThemes();

        // Create model with URI matching the workspace file path
        const modelUri = monaco.Uri.parse(this.options.fileName);
        const model = monaco.editor.createModel(
          this.options.initialCode,
          "python",
          modelUri,
        );

        this.editor = monaco.editor.create(this.editorContainer, {
          model: model,
          theme: this.options.theme === "light" ? "Ayu-Light" : "Ayu-Dark",
          minimap: { enabled: false },
          fontSize: 14,
          scrollBeyondLastLine: false,
          roundedSelection: false,
          contextmenu: true,
          automaticLayout: true,
          fixedOverflowWidgets: true,
        });

        // Setup language server features
        this.languageServer = new LanguageServer(
          this.workspace,
          this.fileHandle,
        );

        // Listen to content changes
        this.editor.onDidChangeModelContent(() => {
          this.onContentChange();
        });

        // Initial check
        this.checkFile();
      }
    } catch (err) {
      this.showError(this.formatError(err));
    }
  }

  private createContainerStructure() {
    const isLight = this.options.theme === "light";

    this.container.style.height = this.options.height;
    this.container.style.display = "flex";
    this.container.style.flexDirection = "column";
    this.container.style.border = isLight
      ? "1px solid #6b7d8f1f"
      : "1px solid #11151c";
    this.container.style.borderRadius = "4px";
    this.container.style.overflow = "hidden";
    this.container.style.fontFamily =
      'Roboto Mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace';

    // Editor container
    this.editorContainer = document.createElement("div");
    this.editorContainer.style.height = this.options.showDiagnostics
      ? `calc(${this.options.height} - 120px)`
      : this.options.height;
    this.container.appendChild(this.editorContainer);

    // Diagnostics container
    if (this.options.showDiagnostics) {
      this.diagnosticsContainer = document.createElement("div");
      this.diagnosticsContainer.style.height = "120px";
      this.diagnosticsContainer.style.overflow = "auto";
      this.diagnosticsContainer.style.borderTop = isLight
        ? "1px solid #6b7d8f1f"
        : "1px solid #11151c";
      this.diagnosticsContainer.style.backgroundColor = isLight
        ? "#f8f9fa"
        : "#0b0e14";
      this.diagnosticsContainer.style.color = isLight ? "#5c6166" : "#bfbdb6";
      this.diagnosticsContainer.style.padding = "8px";
      this.diagnosticsContainer.style.fontSize = "13px";
      this.container.appendChild(this.diagnosticsContainer);
    }
  }

  private onContentChange() {
    // Debounce both workspace update and type checking
    if (this.checkTimeoutId !== null) {
      window.clearTimeout(this.checkTimeoutId);
    }
    this.checkTimeoutId = window.setTimeout(() => {
      const content = this.editor?.getValue() ?? "";

      if (this.workspace && this.fileHandle) {
        try {
          this.workspace.updateFile(this.fileHandle, content);
        } catch (err) {
          console.error("Error updating file:", err);
        }
      }

      this.checkFile();
    }, 150);
  }

  private checkFile() {
    if (!this.workspace || !this.fileHandle || !this.editor) {
      return;
    }

    try {
      const diagnostics = this.workspace.checkFile(this.fileHandle);
      const mapped: Diagnostic[] = diagnostics.map((diagnostic) => ({
        id: diagnostic.id(),
        message: diagnostic.message(),
        severity: diagnostic.severity(),
        range: diagnostic.toRange(this.workspace!) ?? null,
        raw: diagnostic,
      }));

      this.updateDiagnostics(mapped);
      this.hideError();
    } catch (err) {
      console.error("Error checking file:", err);
      this.showError(this.formatError(err));
    }
  }

  private updateDiagnostics(diagnostics: Diagnostic[]) {
    // Update language server diagnostics for code actions
    if (this.languageServer) {
      this.languageServer.updateDiagnostics(diagnostics);
    }

    // Update Monaco markers
    if (this.editor) {
      const model = this.editor.getModel();
      if (model) {
        monaco.editor.setModelMarkers(
          model,
          "owner",
          diagnostics.map((diagnostic) => {
            const range = diagnostic.range;
            return {
              code: diagnostic.id,
              startLineNumber: range?.start?.line ?? 1,
              startColumn: range?.start?.column ?? 1,
              endLineNumber: range?.end?.line ?? 1,
              endColumn: range?.end?.column ?? 1,
              message: diagnostic.message,
              severity: this.mapSeverity(diagnostic.severity),
              tags: [],
            };
          }),
        );
      }
    }

    // Update diagnostics panel
    if (this.diagnosticsContainer) {
      const isLight = this.options.theme === "light";
      const mutedColor = isLight ? "#8a9199" : "#565b66";

      this.diagnosticsContainer.innerHTML = "";

      if (diagnostics.length > 0) {
        const list = document.createElement("ul");
        list.style.margin = "0";
        list.style.padding = "0";
        list.style.listStyle = "none";

        diagnostics.forEach((diagnostic) => {
          const item = this.createDiagnosticItem(diagnostic, mutedColor);
          list.appendChild(item);
        });
        this.diagnosticsContainer.appendChild(list);
      }
    }
  }

  private createDiagnosticItem(
    diagnostic: Diagnostic,
    mutedColor: string,
  ): HTMLElement {
    const startLine = diagnostic.range?.start?.line ?? 1;
    const startColumn = diagnostic.range?.start?.column ?? 1;
    const isLight = this.options.theme === "light";

    // Error: red, Warning: yellow/orange
    const severityColor =
      diagnostic.severity === Severity.Error
        ? isLight
          ? "#e65050"
          : "#d95757"
        : diagnostic.severity === Severity.Warning
          ? isLight
            ? "#f2ae49"
            : "#e6b450"
          : mutedColor;

    const item = document.createElement("li");
    item.style.marginBottom = "8px";
    item.style.paddingLeft = "8px";
    item.style.borderLeft = `3px solid ${severityColor}`;

    const button = document.createElement("button");
    button.style.all = "unset";
    button.style.width = "100%";
    button.style.textAlign = "left";
    button.style.cursor = "pointer";
    button.style.userSelect = "text";

    button.innerHTML = `
      <span style="color: ${severityColor}; font-weight: 500;">${diagnostic.id}</span>
      <span style="color: ${mutedColor}; margin-left: 8px;">${startLine}:${startColumn}</span>
      <div style="margin-top: 2px;">${diagnostic.message}</div>
    `;

    button.addEventListener("click", () => {
      if (diagnostic.range && this.editor) {
        const range = diagnostic.range;
        this.editor.revealRange({
          startLineNumber: range.start.line,
          startColumn: range.start.column,
          endLineNumber: range.end.line,
          endColumn: range.end.column,
        });
        this.editor.setSelection({
          startLineNumber: range.start.line,
          startColumn: range.start.column,
          endLineNumber: range.end.line,
          endColumn: range.end.column,
        });
        this.editor.focus();
      }
    });

    item.appendChild(button);
    return item;
  }

  private showError(message: string) {
    if (!this.errorContainer) {
      this.errorContainer = document.createElement("div");
      this.errorContainer.style.position = "absolute";
      this.errorContainer.style.bottom = "10px";
      this.errorContainer.style.left = "10px";
      this.errorContainer.style.right = "10px";
      this.errorContainer.style.padding = "12px";
      this.errorContainer.style.backgroundColor = "#ff4444";
      this.errorContainer.style.color = "#fff";
      this.errorContainer.style.borderRadius = "4px";
      this.container.style.position = "relative";
      this.container.appendChild(this.errorContainer);
    }
    this.errorContainer.textContent = message;
    this.errorContainer.style.display = "block";
  }

  private hideError() {
    if (this.errorContainer) {
      this.errorContainer.style.display = "none";
    }
  }

  private formatError(error: unknown): string {
    const message = error instanceof Error ? error.message : `${error}`;
    return message.startsWith("Error: ")
      ? message.slice("Error: ".length)
      : message;
  }

  private mapSeverity(severity: Severity): monaco.MarkerSeverity {
    switch (severity) {
      case Severity.Info:
        return monaco.MarkerSeverity.Info;
      case Severity.Warning:
        return monaco.MarkerSeverity.Warning;
      case Severity.Error:
        return monaco.MarkerSeverity.Error;
      case Severity.Fatal:
        return monaco.MarkerSeverity.Error;
    }
  }

  dispose() {
    this.languageServer?.dispose();
    this.editor?.dispose();
    if (this.workspace && this.fileHandle) {
      try {
        this.workspace.closeFile(this.fileHandle);
      } catch (err) {
        console.warn("Error closing file:", err);
      }
    }
    if (this.checkTimeoutId !== null) {
      window.clearTimeout(this.checkTimeoutId);
    }
  }
}

class LanguageServer
  implements
    monaco.languages.DefinitionProvider,
    monaco.languages.HoverProvider,
    monaco.languages.CompletionItemProvider,
    monaco.languages.InlayHintsProvider,
    monaco.languages.CodeActionProvider
{
  private definitionDisposable: monaco.IDisposable;
  private hoverDisposable: monaco.IDisposable;
  private completionDisposable: monaco.IDisposable;
  private inlayHintsDisposable: monaco.IDisposable;
  private codeActionDisposable: monaco.IDisposable;
  private diagnostics: Diagnostic[] = [];

  constructor(
    private workspace: Workspace,
    private fileHandle: FileHandle,
  ) {
    this.definitionDisposable = monaco.languages.registerDefinitionProvider(
      "python",
      this,
    );
    this.hoverDisposable = monaco.languages.registerHoverProvider(
      "python",
      this,
    );
    this.completionDisposable =
      monaco.languages.registerCompletionItemProvider("python", this);
    this.inlayHintsDisposable = monaco.languages.registerInlayHintsProvider(
      "python",
      this,
    );
    this.codeActionDisposable = monaco.languages.registerCodeActionProvider(
      "python",
      this,
    );
  }

  updateDiagnostics(diagnostics: Diagnostic[]) {
    this.diagnostics = diagnostics;
  }

  triggerCharacters = ["."];

  provideCompletionItems(
    _model: monaco.editor.ITextModel,
    position: monaco.Position,
  ): monaco.languages.ProviderResult<monaco.languages.CompletionList> {
    try {
      const completions = this.workspace.completions(
        this.fileHandle,
        new TyPosition(position.lineNumber, position.column),
      );

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
              ? monaco.languages.CompletionItemKind.Variable
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
          range: undefined as any,
        })),
      };
    } catch (err) {
      console.warn("Error providing completions:", err);
      return undefined;
    }
  }

  provideHover(
    _model: monaco.editor.ITextModel,
    position: monaco.Position,
  ): monaco.languages.ProviderResult<monaco.languages.Hover> {
    try {
      const hover = this.workspace.hover(
        this.fileHandle,
        new TyPosition(position.lineNumber, position.column),
      );

      if (hover == null) {
        return undefined;
      }

      return {
        range: tyRangeToMonacoRange(hover.range),
        contents: [{ value: hover.markdown, isTrusted: true }],
      };
    } catch (err) {
      console.warn("Error providing hover:", err);
      return undefined;
    }
  }

  provideDefinition(
    model: monaco.editor.ITextModel,
    position: monaco.Position,
  ): monaco.languages.ProviderResult<
    monaco.languages.Definition | monaco.languages.LocationLink[]
  > {
    try {
      const links = this.workspace.gotoDefinition(
        this.fileHandle,
        new TyPosition(position.lineNumber, position.column),
      );

      if (links.length === 0) {
        return undefined;
      }

      const currentUri = model.uri;
      const results = links
        .filter((link: LocationLink) => {
          const linkUri = monaco.Uri.parse(link.path);
          return linkUri.path === currentUri.path;
        })
        .map((link: LocationLink) => ({
          uri: currentUri,
          range: tyRangeToMonacoRange(link.full_range),
          targetSelectionRange:
            link.selection_range == null
              ? undefined
              : tyRangeToMonacoRange(link.selection_range),
          originSelectionRange:
            link.origin_selection_range == null
              ? undefined
              : tyRangeToMonacoRange(link.origin_selection_range),
        }));

      return results.length > 0 ? results : undefined;
    } catch (err) {
      console.warn("Error providing definition:", err);
      return undefined;
    }
  }

  provideInlayHints(
    _model: monaco.editor.ITextModel,
    range: monaco.IRange,
  ): monaco.languages.ProviderResult<monaco.languages.InlayHintList> {
    try {
      const inlayHints = this.workspace.inlayHints(
        this.fileHandle,
        monacoRangeToTyRange(range),
      );

      if (inlayHints.length === 0) {
        return undefined;
      }

      return {
        dispose: () => {},
        hints: inlayHints.map((hint) => ({
          label: hint.label.map((part) => ({
            label: part.label,
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
    } catch (err) {
      console.warn("Error providing inlay hints:", err);
      return undefined;
    }
  }

  provideCodeActions(
    model: monaco.editor.ITextModel,
    range: monaco.Range,
  ): monaco.languages.ProviderResult<monaco.languages.CodeActionList> {
    const actions: monaco.languages.CodeAction[] = [];

    for (const diagnostic of this.diagnostics) {
      const diagnosticRange = diagnostic.range;
      if (diagnosticRange == null) {
        continue;
      }

      const monacoRange = tyRangeToMonacoRange(diagnosticRange);
      if (!monaco.Range.areIntersecting(range, new monaco.Range(
        monacoRange.startLineNumber,
        monacoRange.startColumn,
        monacoRange.endLineNumber,
        monacoRange.endColumn,
      ))) {
        continue;
      }

      try {
        const codeActions = this.workspace.codeActions(
          this.fileHandle,
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
      } catch (err) {
        console.warn("Error getting code actions:", err);
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

  dispose() {
    this.definitionDisposable.dispose();
    this.hoverDisposable.dispose();
    this.completionDisposable.dispose();
    this.inlayHintsDisposable.dispose();
    this.codeActionDisposable.dispose();
  }
}

// Helper functions
function tyRangeToMonacoRange(range: TyRange): monaco.IRange {
  return {
    startLineNumber: range.start.line,
    startColumn: range.start.column,
    endLineNumber: range.end.line,
    endColumn: range.end.column,
  };
}

function monacoRangeToTyRange(range: monaco.IRange): TyRange {
  return new TyRange(
    new TyPosition(range.startLineNumber, range.startColumn),
    new TyPosition(range.endLineNumber, range.endColumn),
  );
}

function mapInlayHintKind(kind: InlayHintKind): monaco.languages.InlayHintKind {
  switch (kind) {
    case InlayHintKind.Type:
      return monaco.languages.InlayHintKind.Type;
    case InlayHintKind.Parameter:
      return monaco.languages.InlayHintKind.Parameter;
  }
}

function mapCompletionKind(
  kind: CompletionKind,
): monaco.languages.CompletionItemKind {
  switch (kind) {
    case CompletionKind.Text:
      return monaco.languages.CompletionItemKind.Text;
    case CompletionKind.Method:
      return monaco.languages.CompletionItemKind.Method;
    case CompletionKind.Function:
      return monaco.languages.CompletionItemKind.Function;
    case CompletionKind.Constructor:
      return monaco.languages.CompletionItemKind.Constructor;
    case CompletionKind.Field:
      return monaco.languages.CompletionItemKind.Field;
    case CompletionKind.Variable:
      return monaco.languages.CompletionItemKind.Variable;
    case CompletionKind.Class:
      return monaco.languages.CompletionItemKind.Class;
    case CompletionKind.Interface:
      return monaco.languages.CompletionItemKind.Interface;
    case CompletionKind.Module:
      return monaco.languages.CompletionItemKind.Module;
    case CompletionKind.Property:
      return monaco.languages.CompletionItemKind.Property;
    case CompletionKind.Unit:
      return monaco.languages.CompletionItemKind.Unit;
    case CompletionKind.Value:
      return monaco.languages.CompletionItemKind.Value;
    case CompletionKind.Enum:
      return monaco.languages.CompletionItemKind.Enum;
    case CompletionKind.Keyword:
      return monaco.languages.CompletionItemKind.Keyword;
    case CompletionKind.Snippet:
      return monaco.languages.CompletionItemKind.Snippet;
    case CompletionKind.Color:
      return monaco.languages.CompletionItemKind.Color;
    case CompletionKind.File:
      return monaco.languages.CompletionItemKind.File;
    case CompletionKind.Reference:
      return monaco.languages.CompletionItemKind.Reference;
    case CompletionKind.Folder:
      return monaco.languages.CompletionItemKind.Folder;
    case CompletionKind.EnumMember:
      return monaco.languages.CompletionItemKind.EnumMember;
    case CompletionKind.Constant:
      return monaco.languages.CompletionItemKind.Constant;
    case CompletionKind.Struct:
      return monaco.languages.CompletionItemKind.Struct;
    case CompletionKind.Event:
      return monaco.languages.CompletionItemKind.Event;
    case CompletionKind.Operator:
      return monaco.languages.CompletionItemKind.Operator;
    case CompletionKind.TypeParameter:
      return monaco.languages.CompletionItemKind.TypeParameter;
  }
}


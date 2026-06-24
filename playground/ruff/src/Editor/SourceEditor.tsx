/**
 * Editor for the Python source code.
 */

import MonacoEditor, { Monaco, OnMount } from "@monaco-editor/react";
import {
  editor,
  IDisposable,
  languages,
  MarkerSeverity,
  MarkerTag,
  Range,
} from "monaco-editor";
import { useCallback, useEffect, useRef } from "react";
import type { Diagnostic, DiagnosticLocation, DiagnosticTag } from "ruff_wasm";
import { secondaryAnnotationsWithMessages, Theme } from "shared";
import CodeActionProvider = languages.CodeActionProvider;
import IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

export const PLAYGROUND_FILE_PATH = "<filename>";

const markerTagByDiagnosticTag = {
  unnecessary: MarkerTag.Unnecessary,
  deprecated: MarkerTag.Deprecated,
} satisfies Record<DiagnosticTag, MarkerTag>;

type MonacoEditorState = {
  monaco: Monaco;
  codeActionProvider: RuffCodeActionProvider;
  disposeCodeActionProvider: IDisposable;
};

export default function SourceEditor({
  visible,
  source,
  theme,
  diagnostics,
  onChange,
  onMount,
}: {
  visible: boolean;
  source: string;
  diagnostics: Diagnostic[];
  theme: Theme;
  onChange(pythonSource: string): void;
  onMount(editor: IStandaloneCodeEditor): void;
}) {
  const monacoRef = useRef<MonacoEditorState | null>(null);

  // Update the diagnostics in the editor.
  useEffect(() => {
    const editorState = monacoRef.current;

    if (editorState == null) {
      return;
    }

    editorState.codeActionProvider.diagnostics = diagnostics;

    updateMarkers(editorState.monaco, diagnostics);
  }, [diagnostics]);

  // Dispose the code action provider on unmount.
  useEffect(() => {
    const disposeActionProvider = monacoRef.current?.disposeCodeActionProvider;
    if (disposeActionProvider == null) {
      return;
    }

    return () => {
      disposeActionProvider.dispose();
    };
  }, []);

  const handleChange = useCallback(
    (value: string | undefined) => {
      onChange(value ?? "");
    },
    [onChange],
  );

  const handleMount: OnMount = useCallback(
    (editor, instance) => {
      const ruffActionsProvider = new RuffCodeActionProvider(diagnostics);
      const disposeCodeActionProvider =
        instance.languages.registerCodeActionProvider(
          "python",
          ruffActionsProvider,
        );

      updateMarkers(instance, diagnostics);

      monacoRef.current = {
        monaco: instance,
        codeActionProvider: ruffActionsProvider,
        disposeCodeActionProvider,
      };

      onMount(editor);
    },

    [diagnostics, onMount],
  );

  return (
    <MonacoEditor
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
      language={"python"}
      wrapperProps={visible ? {} : { style: { display: "none" } }}
      theme={theme === "light" ? "Ayu-Light" : "Ayu-Dark"}
      value={source}
      onChange={handleChange}
    />
  );
}

class RuffCodeActionProvider implements CodeActionProvider {
  constructor(public diagnostics: Array<Diagnostic>) {}

  provideCodeActions(
    model: editor.ITextModel,
    range: Range,
  ): languages.ProviderResult<languages.CodeActionList> {
    const actions = this.diagnostics
      // Show fixes for any diagnostic whose range intersects the requested range
      .filter((check) => {
        const diagnosticRange = new Range(
          check.start_location.row,
          check.start_location.column,
          check.end_location.row,
          check.end_location.column,
        );

        return Range.areIntersectingOrTouching(diagnosticRange, range);
      })
      .filter(({ fix }) => fix)
      .map((check) => ({
        title: check.fix
          ? check.fix.message
            ? `${check.code}: ${check.fix.message}`
            : `Fix ${check.code}`
          : "Fix",
        id: `fix-${check.code}`,
        kind: "quickfix",

        edit: check.fix
          ? {
              edits: check.fix.edits.map((edit) => ({
                resource: model.uri,
                versionId: model.getVersionId(),
                textEdit: {
                  range: {
                    startLineNumber: edit.location.row,
                    startColumn: edit.location.column,
                    endLineNumber: edit.end_location.row,
                    endColumn: edit.end_location.column,
                  },
                  text: edit.content || "",
                },
              })),
            }
          : undefined,
      }));

    return {
      actions,
      dispose: () => {},
    };
  }
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
      const message = diagnosticMarkerMessage(diagnostic);

      return {
        code: diagnostic.code ?? undefined,
        startLineNumber: diagnostic.start_location.row,
        startColumn: diagnostic.start_location.column,
        endLineNumber: diagnostic.end_location.row,
        endColumn: diagnostic.end_location.column,
        message: diagnostic.code ? `${diagnostic.code}: ${message}` : message,
        relatedInformation: diagnosticRelatedInformation(diagnostic, model.uri),
        severity: MarkerSeverity.Error,
        tags: diagnostic.tags.map((tag) => markerTagByDiagnosticTag[tag]),
      };
    }),
  );
}

function diagnosticMarkerMessage(diagnostic: Diagnostic): string {
  // Monaco renders same-file subdiagnostics as related information. Keep
  // unlocated and other-file subdiagnostics in the marker message instead.
  const markerMessageSubDiagnostics = diagnostic.subDiagnostics.filter(
    (subDiagnostic) => subDiagnostic.location?.path !== PLAYGROUND_FILE_PATH,
  );

  if (markerMessageSubDiagnostics.length === 0) {
    return diagnostic.message;
  }

  return `${diagnostic.message}\n\n${markerMessageSubDiagnostics.map(formatSubDiagnostic).join("\n")}`;
}

function diagnosticRelatedInformation(
  diagnostic: Diagnostic,
  resource: editor.ITextModel["uri"],
): editor.IRelatedInformation[] {
  const secondaryAnnotations = secondaryAnnotationsWithMessages(
    diagnostic.annotations,
  ).flatMap((annotation) =>
    diagnosticLocationRelatedInformation(
      annotation.message,
      annotation.location,
      resource,
    ),
  );

  const subDiagnostics = diagnostic.subDiagnostics.flatMap((subDiagnostic) =>
    diagnosticLocationRelatedInformation(
      formatSubDiagnostic(subDiagnostic),
      subDiagnostic.location,
      resource,
    ),
  );

  return secondaryAnnotations.concat(subDiagnostics);
}

function diagnosticLocationRelatedInformation(
  message: string,
  location: DiagnosticLocation | null,
  resource: editor.ITextModel["uri"],
): editor.IRelatedInformation[] {
  if (location?.path !== PLAYGROUND_FILE_PATH) {
    return [];
  }

  return [
    {
      resource,
      message,
      startLineNumber: location.start_location.row,
      startColumn: location.start_location.column,
      endLineNumber: location.end_location.row,
      endColumn: location.end_location.column,
    },
  ];
}

function formatSubDiagnostic(
  subDiagnostic: Diagnostic["subDiagnostics"][number],
): string {
  const message = `${subDiagnostic.severity}: ${subDiagnostic.message}`;
  const location = subDiagnostic.location;

  if (location == null || location.path === PLAYGROUND_FILE_PATH) {
    return message;
  }

  return `${message} (${location.path}:${location.start_location.row}:${location.start_location.column})`;
}

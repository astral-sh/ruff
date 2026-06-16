import { SubDiagnosticSeverity } from "ty_wasm";
import type {
  DiagnosticAnnotation,
  Severity,
  Range,
  SubDiagnostic,
  TextRange,
  Diagnostic as TyDiagnostic,
} from "ty_wasm";
import classNames from "classnames";
import {
  type DiagnosticDetail,
  type DiagnosticDetailLocation,
  DiagnosticDetailItem,
  secondaryAnnotationsWithMessages,
  Theme,
} from "shared";
import { useMemo } from "react";

interface Props {
  diagnostics: Diagnostic[];
  currentFilePath: string | null;
  theme: Theme;

  onGoTo(location: DiagnosticDetailLocation): void;
}

export default function Diagnostics({
  diagnostics: unsorted,
  currentFilePath,
  theme,
  onGoTo,
}: Props) {
  const diagnostics = useMemo(() => {
    const sorted = [...unsorted];
    sorted.sort((a, b) => {
      return (a.textRange?.start ?? 0) - (b.textRange?.start ?? 0);
    });

    return sorted;
  }, [unsorted]);

  return (
    <div
      className={classNames(
        "flex h-full min-h-0 flex-col overflow-hidden",
        theme === "dark" ? "text-white" : null,
      )}
    >
      <div
        className={classNames(
          "border-b border-gray-200 px-2 py-1",
          theme === "dark" ? "border-rock" : null,
        )}
      >
        File diagnostics ({diagnostics.length})
      </div>

      <div className="flex min-h-0 grow overflow-hidden p-2">
        <Items
          diagnostics={diagnostics}
          currentFilePath={currentFilePath}
          onGoTo={onGoTo}
        />
      </div>
    </div>
  );
}

function Items({
  diagnostics,
  currentFilePath,
  onGoTo,
}: {
  diagnostics: Array<Diagnostic>;
  currentFilePath: string | null;
  onGoTo(location: DiagnosticDetailLocation): void;
}) {
  if (diagnostics.length === 0) {
    return (
      <div className={"flex flex-auto flex-col justify-center  items-center"}>
        Everything is looking good!
      </div>
    );
  }

  const uniqueIds: Map<string, number> = new Map();

  return (
    <ul className="space-y-0.5 grow overflow-y-auto">
      {diagnostics.map((diagnostic) => {
        const position = diagnostic.range;
        const start = position?.start;
        const id = diagnostic.id;

        const startLine = start?.line ?? 1;
        const startColumn = start?.column ?? 1;
        const location =
          currentFilePath == null || position == null
            ? null
            : {
                path: currentFilePath,
                startLineNumber: startLine,
                startColumn: startColumn,
                endLineNumber: position.end.line,
                endColumn: position.end.column,
              };
        const secondaryAnnotations = secondaryAnnotationsWithMessages(
          diagnostic.annotations,
        );

        const mostlyUniqueId = `${startLine}:${startColumn}-${id}`;

        const disambiguator = uniqueIds.get(mostlyUniqueId) ?? 0;
        uniqueIds.set(mostlyUniqueId, disambiguator + 1);

        return (
          <li key={`${mostlyUniqueId}-${disambiguator}`}>
            {location == null ? (
              <span className="w-full text-start select-text">
                {diagnostic.message}
                <span className="text-gray-500">
                  {id != null && ` (${id})`} [Ln {startLine}, Col {startColumn}]
                </span>
              </span>
            ) : (
              <button
                onClick={() => onGoTo(location)}
                className="w-full text-start cursor-pointer select-text"
              >
                {diagnostic.message}
                <span className="text-gray-500">
                  {id != null && ` (${id})`} [Ln {startLine}, Col {startColumn}]
                </span>
              </button>
            )}
            {/* Some subdiagnostics use whitespace to align types in columns, so
                we use a monospace font. See
                https://github.com/astral-sh/ruff/pull/25860#pullrequestreview-4475222305 */}
            {secondaryAnnotations.length > 0 ||
            diagnostic.subDiagnostics.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {secondaryAnnotations.map((annotation, index) => {
                  const location = annotation.location ?? null;

                  return (
                    <li key={`annotation-${index}`}>
                      <DiagnosticDetailItem
                        item={toDisplayDiagnosticDetail({
                          message: annotation.message,
                          location,
                        })}
                        onGoTo={onGoTo}
                      />
                    </li>
                  );
                })}
                {diagnostic.subDiagnostics.map((subDiagnostic, index) => (
                  <li key={`sub-diagnostic-${index}`}>
                    <SubDiagnosticItem
                      subDiagnostic={subDiagnostic}
                      onGoTo={onGoTo}
                    />
                  </li>
                ))}
              </ul>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

export interface Diagnostic {
  id: string;
  message: string;
  annotations: DiagnosticAnnotation[];
  subDiagnostics: SubDiagnostic[];
  severity: Severity;
  range: Range | null;
  textRange: TextRange | null;
  raw: TyDiagnostic;
}

export type DiagnosticLocation = {
  path: string;
  range: Range;
};

function SubDiagnosticItem({
  subDiagnostic,
  onGoTo,
}: {
  subDiagnostic: SubDiagnostic;
  onGoTo(location: DiagnosticDetailLocation): void;
}) {
  let primaryAnnotation: DiagnosticAnnotation | undefined;
  const additionalAnnotations: DiagnosticAnnotation[] = [];

  for (const annotation of subDiagnostic.annotations) {
    if (annotation.primary && primaryAnnotation == null) {
      primaryAnnotation = annotation;
    } else {
      additionalAnnotations.push(annotation);
    }
  }

  const severity = formatSubDiagnosticSeverity(subDiagnostic.severity);
  const primaryLocation = primaryAnnotation?.location ?? null;

  return (
    <>
      {primaryAnnotation == null ? (
        <DiagnosticDetailItem
          item={{ message: subDiagnostic.message, severity, location: null }}
        />
      ) : (
        <DiagnosticDetailItem
          item={toDisplayDiagnosticDetail({
            message: formatSubDiagnosticAnnotation(
              subDiagnostic,
              primaryAnnotation,
            ),
            location: primaryLocation,
            severity,
          })}
          onGoTo={onGoTo}
        />
      )}
      {additionalAnnotations.length > 0 ? (
        <ul className="pl-3">
          {additionalAnnotations.map((annotation, index) => {
            const location = annotation.location ?? null;

            return (
              <li key={index}>
                <DiagnosticDetailItem
                  item={toDisplayDiagnosticDetail({
                    message: formatSubDiagnosticAnnotation(
                      subDiagnostic,
                      annotation,
                      false,
                    ),
                    location,
                  })}
                  onGoTo={onGoTo}
                />
              </li>
            );
          })}
        </ul>
      ) : null}
    </>
  );
}

function toDisplayDiagnosticDetail(item: {
  message: string;
  severity?: string;
  location: DiagnosticLocation | null;
}): DiagnosticDetail {
  const location = item.location;

  return {
    message: item.message,
    severity: item.severity,
    location:
      location == null
        ? null
        : {
            path: location.path,
            startLineNumber: location.range.start.line,
            startColumn: location.range.start.column,
            endLineNumber: location.range.end.line,
            endColumn: location.range.end.column,
          },
  };
}

export function formatSubDiagnostic(subDiagnostic: SubDiagnostic): string {
  return `${formatSubDiagnosticSeverity(subDiagnostic.severity)}: ${subDiagnostic.message}`;
}

export function formatSubDiagnosticAnnotation(
  subDiagnostic: SubDiagnostic,
  annotation: DiagnosticAnnotation,
  includeSubDiagnosticMessage = annotation.primary,
): string {
  if (annotation.message == null) {
    return subDiagnostic.message;
  }

  return includeSubDiagnosticMessage
    ? `${subDiagnostic.message}: ${annotation.message}`
    : annotation.message;
}

function formatSubDiagnosticSeverity(severity: SubDiagnosticSeverity): string {
  switch (severity) {
    case SubDiagnosticSeverity.Help:
      return "help";
    case SubDiagnosticSeverity.Info:
      return "info";
    case SubDiagnosticSeverity.Warning:
      return "warning";
    case SubDiagnosticSeverity.Error:
      return "error";
    case SubDiagnosticSeverity.Fatal:
      return "fatal";
  }
}

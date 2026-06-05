import { SubDiagnosticSeverity } from "ty_wasm";
import type {
  Severity,
  Range,
  SubDiagnostic,
  SubDiagnosticAnnotation,
  TextRange,
  Diagnostic as TyDiagnostic,
} from "ty_wasm";
import classNames from "classnames";
import { Theme } from "shared";
import { useMemo } from "react";

interface Props {
  diagnostics: Diagnostic[];
  currentFilePath: string | null;
  theme: Theme;

  onGoTo(location: DiagnosticLocation): void;
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
  onGoTo(location: DiagnosticLocation): void;
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
            : { path: currentFilePath, range: position };

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
            {diagnostic.subDiagnostics.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {diagnostic.subDiagnostics.map((subDiagnostic, index) => (
                  <li key={index}>
                    <SubDiagnosticItem
                      subDiagnostic={subDiagnostic}
                      currentFilePath={currentFilePath}
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
  currentFilePath,
  onGoTo,
}: {
  subDiagnostic: SubDiagnostic;
  currentFilePath: string | null;
  onGoTo(location: DiagnosticLocation): void;
}) {
  let primaryAnnotation: SubDiagnosticAnnotation | undefined;
  const additionalAnnotations: SubDiagnosticAnnotation[] = [];

  for (const annotation of subDiagnostic.annotations) {
    if (annotation.primary && primaryAnnotation == null) {
      primaryAnnotation = annotation;
    } else {
      additionalAnnotations.push(annotation);
    }
  }

  return (
    <>
      {primaryAnnotation == null ? (
        <span>{formatSubDiagnostic(subDiagnostic)}</span>
      ) : (
        <SubDiagnosticAnnotationItem
          prefix={`${formatSubDiagnosticSeverity(subDiagnostic.severity)}: `}
          message={formatSubDiagnosticAnnotation(
            subDiagnostic,
            primaryAnnotation,
          )}
          annotation={primaryAnnotation}
          currentFilePath={currentFilePath}
          onGoTo={onGoTo}
        />
      )}
      {additionalAnnotations.length > 0 ? (
        <ul className="pl-3">
          {additionalAnnotations.map((annotation, index) => (
            <li key={index}>
              <SubDiagnosticAnnotationItem
                message={formatSubDiagnosticAnnotation(
                  subDiagnostic,
                  annotation,
                  false,
                )}
                annotation={annotation}
                currentFilePath={currentFilePath}
                onGoTo={onGoTo}
              />
            </li>
          ))}
        </ul>
      ) : null}
    </>
  );
}

function SubDiagnosticAnnotationItem({
  prefix,
  message,
  annotation,
  currentFilePath,
  onGoTo,
}: {
  prefix?: string;
  message: string;
  annotation: SubDiagnosticAnnotation;
  currentFilePath: string | null;
  onGoTo(location: DiagnosticLocation): void;
}) {
  const location = annotation.location;
  if (location == null) {
    return (
      <span>
        {prefix}
        {message}
      </span>
    );
  }

  const start = location.range.start;
  const locationLabel =
    location.path === currentFilePath
      ? `[Ln ${start.line}, Col ${start.column}]`
      : `[${location.path}: Ln ${start.line}, Col ${start.column}]`;

  return (
    <>
      {prefix}
      <button
        onClick={() => onGoTo(location)}
        className="text-start cursor-pointer text-current underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
      >
        {message}
        <span className="text-gray-500"> {locationLabel}</span>
      </button>
    </>
  );
}

export function formatSubDiagnostic(subDiagnostic: SubDiagnostic): string {
  return `${formatSubDiagnosticSeverity(subDiagnostic.severity)}: ${subDiagnostic.message}`;
}

export function formatSubDiagnosticAnnotation(
  subDiagnostic: SubDiagnostic,
  annotation: SubDiagnosticAnnotation,
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

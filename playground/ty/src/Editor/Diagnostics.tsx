import type {
  Severity,
  Location as TyLocation,
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

  onGoTo(line: number, column: number): void;
  onGoToLocation(location: DiagnosticLocation): void;
}

export default function Diagnostics({
  diagnostics: unsorted,
  currentFilePath,
  theme,
  onGoTo,
  onGoToLocation,
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
          onGoToLocation={onGoToLocation}
        />
      </div>
    </div>
  );
}

function Items({
  diagnostics,
  currentFilePath,
  onGoTo,
  onGoToLocation,
}: {
  diagnostics: Array<Diagnostic>;
  currentFilePath: string | null;
  onGoTo(line: number, column: number): void;
  onGoToLocation(location: DiagnosticLocation): void;
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

        const mostlyUniqueId = `${startLine}:${startColumn}-${id}`;

        const disambiguator = uniqueIds.get(mostlyUniqueId) ?? 0;
        uniqueIds.set(mostlyUniqueId, disambiguator + 1);

        return (
          <li key={`${mostlyUniqueId}-${disambiguator}`}>
            <button
              onClick={() => onGoTo(startLine, startColumn)}
              className="w-full text-start cursor-pointer select-text"
            >
              {diagnostic.message}
              <span className="text-gray-500">
                {id != null && ` (${id})`} [Ln {startLine}, Col {startColumn}]
              </span>
            </button>
            {diagnostic.subDiagnostics.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {diagnostic.subDiagnostics.map((subDiagnostic, index) => (
                  <li key={index}>
                    <SubDiagnosticItem
                      subDiagnostic={subDiagnostic}
                      currentFilePath={currentFilePath}
                      onGoToLocation={onGoToLocation}
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

export type DiagnosticLocation = Pick<TyLocation, "path" | "range">;

function SubDiagnosticItem({
  subDiagnostic,
  currentFilePath,
  onGoToLocation,
}: {
  subDiagnostic: SubDiagnostic;
  currentFilePath: string | null;
  onGoToLocation(location: DiagnosticLocation): void;
}) {
  const annotations = subDiagnostic.annotations;
  const primaryAnnotationIndex = subDiagnostic.primary_annotation_index;
  const primaryAnnotation =
    primaryAnnotationIndex == null
      ? undefined
      : annotations[primaryAnnotationIndex];
  const additionalAnnotations = annotations.filter(
    (_, index) => index !== primaryAnnotationIndex,
  );

  return (
    <>
      {primaryAnnotation == null ? (
        <span>{formatSubDiagnostic(subDiagnostic)}</span>
      ) : (
        <SubDiagnosticAnnotationItem
          prefix={`${subDiagnostic.severity}: `}
          message={formatPrimaryAnnotation(subDiagnostic, primaryAnnotation)}
          annotation={primaryAnnotation}
          currentFilePath={currentFilePath}
          onGoToLocation={onGoToLocation}
        />
      )}
      {additionalAnnotations.length > 0 ? (
        <ul className="pl-3">
          {additionalAnnotations.map((annotation, index) => (
            <li key={index}>
              <SubDiagnosticAnnotationItem
                message={annotation.message ?? subDiagnostic.message}
                annotation={annotation}
                currentFilePath={currentFilePath}
                onGoToLocation={onGoToLocation}
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
  onGoToLocation,
}: {
  prefix?: string;
  message: string;
  annotation: SubDiagnosticAnnotation;
  currentFilePath: string | null;
  onGoToLocation(location: DiagnosticLocation): void;
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
        onClick={() => onGoToLocation(location)}
        className="text-start cursor-pointer text-current underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
      >
        {message}
        <span className="text-gray-500"> {locationLabel}</span>
      </button>
    </>
  );
}

function formatSubDiagnostic(subDiagnostic: SubDiagnostic): string {
  return `${subDiagnostic.severity}: ${subDiagnostic.message}`;
}

function formatPrimaryAnnotation(
  subDiagnostic: SubDiagnostic,
  annotation: SubDiagnosticAnnotation,
): string {
  return annotation.message == null
    ? subDiagnostic.message
    : `${subDiagnostic.message}: ${annotation.message}`;
}

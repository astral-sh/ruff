import type { Diagnostic, DiagnosticLocation, SubDiagnostic } from "ruff_wasm";
import classNames from "classnames";
import {
  DiagnosticLocationItem,
  renderableSecondaryDiagnosticAnnotations,
  Theme,
} from "shared";
import { useMemo } from "react";
import { PLAYGROUND_FILE_PATH } from "./SourceEditor";

interface Props {
  diagnostics: Diagnostic[];
  theme: Theme;

  onGoTo(line: number, column: number): void;
}

export default function Diagnostics({
  diagnostics: unsorted,
  theme,
  onGoTo,
}: Props) {
  const diagnostics = useMemo(() => {
    const sorted = [...unsorted];
    sorted.sort((a, b) => {
      if (a.start_location.row === b.start_location.row) {
        return a.start_location.column - b.start_location.column;
      }

      return a.start_location.row - b.start_location.row;
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
        Diagnostics ({diagnostics.length})
      </div>

      <div className="flex min-h-0 grow overflow-hidden p-2">
        <Items diagnostics={diagnostics} onGoTo={onGoTo} />
      </div>
    </div>
  );
}

function Items({
  diagnostics,
  onGoTo,
}: {
  diagnostics: Array<Diagnostic>;
  onGoTo(line: number, column: number): void;
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
        const row = diagnostic.start_location.row;
        const column = diagnostic.start_location.column;
        const secondaryAnnotations = renderableSecondaryDiagnosticAnnotations(
          diagnostic.annotations,
        );
        const mostlyUniqueId = `${row}:${column}-${diagnostic.code}`;

        const disambiguator = uniqueIds.get(mostlyUniqueId) ?? 0;
        uniqueIds.set(mostlyUniqueId, disambiguator + 1);

        return (
          <li key={`${mostlyUniqueId}-${disambiguator}`}>
            <button
              onClick={() => onGoTo(row, column)}
              className="w-full text-start cursor-pointer select-text"
            >
              {diagnostic.message}{" "}
              <span className="text-gray-500">
                {diagnostic.code != null && `(${diagnostic.code})`} [Ln {row},
                Col {column}]
              </span>
            </button>
            {secondaryAnnotations.length > 0 ||
            diagnostic.subDiagnostics.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {secondaryAnnotations.map((annotation, index) => (
                  <li key={`annotation-${index}`}>
                    <RuffDiagnosticLocationItem
                      message={annotation.message}
                      location={annotation.location}
                      onGoTo={onGoTo}
                    />
                  </li>
                ))}
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

function RuffDiagnosticLocationItem({
  prefix,
  message,
  location,
  onGoTo,
}: {
  prefix?: string;
  message: string;
  location: DiagnosticLocation | null | undefined;
  onGoTo(line: number, column: number): void;
}) {
  const start = location?.start_location;
  const locationLabel =
    location == null || start == null
      ? undefined
      : location.path === PLAYGROUND_FILE_PATH
        ? `[Ln ${start.row}, Col ${start.column}]`
        : `[${location.path}: Ln ${start.row}, Col ${start.column}]`;

  return (
    <DiagnosticLocationItem
      prefix={prefix}
      message={message}
      locationLabel={locationLabel}
      onGoTo={
        location?.path === PLAYGROUND_FILE_PATH && start != null
          ? () => onGoTo(start.row, start.column)
          : undefined
      }
    />
  );
}

function SubDiagnosticItem({
  subDiagnostic,
  onGoTo,
}: {
  subDiagnostic: SubDiagnostic;
  onGoTo(line: number, column: number): void;
}) {
  return (
    <RuffDiagnosticLocationItem
      prefix={`${subDiagnostic.severity}: `}
      message={subDiagnostic.message}
      location={subDiagnostic.location}
      onGoTo={onGoTo}
    />
  );
}

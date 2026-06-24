import type { Diagnostic, DiagnosticLocation } from "ruff_wasm";
import classNames from "classnames";
import {
  type DiagnosticDetail,
  type DiagnosticDetailLocation,
  DiagnosticDetailItem,
  secondaryAnnotationsWithMessages,
  Theme,
} from "shared";
import { useCallback, useMemo } from "react";
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
  const handleDetailGoTo = useCallback(
    (location: DiagnosticDetailLocation) => {
      onGoTo(location.startLineNumber, location.startColumn);
    },
    [onGoTo],
  );

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
        const secondaryAnnotations = secondaryAnnotationsWithMessages(
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
            {/* Some subdiagnostics use whitespace to align types in columns, so
                we use a monospace font. See
                https://github.com/astral-sh/ruff/pull/25860#pullrequestreview-4475222305 */}
            {secondaryAnnotations.length > 0 ||
            diagnostic.subDiagnostics.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {secondaryAnnotations.map((annotation, index) => (
                  <li key={`annotation-${index}`}>
                    <DiagnosticDetailItem
                      item={toDisplayDiagnosticDetail(annotation)}
                      onGoTo={
                        annotation.location?.path === PLAYGROUND_FILE_PATH
                          ? handleDetailGoTo
                          : undefined
                      }
                    />
                  </li>
                ))}
                {diagnostic.subDiagnostics.map((subDiagnostic, index) => (
                  <li key={`sub-diagnostic-${index}`}>
                    <DiagnosticDetailItem
                      item={toDisplayDiagnosticDetail(subDiagnostic)}
                      onGoTo={
                        subDiagnostic.location?.path === PLAYGROUND_FILE_PATH
                          ? handleDetailGoTo
                          : undefined
                      }
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
            startLineNumber: location.start_location.row,
            startColumn: location.start_location.column,
            endLineNumber: location.end_location.row,
            endColumn: location.end_location.column,
          },
  };
}

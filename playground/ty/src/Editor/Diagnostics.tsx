import type {
  Severity,
  Range,
  TextRange,
  Diagnostic as TyDiagnostic,
} from "ty_wasm";
import classNames from "classnames";
import { Theme } from "shared";
import { useMemo } from "react";

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
      return (a.textRange?.start ?? 0) - (b.textRange?.start ?? 0);
    });

    return sorted;
  }, [unsorted]);

  return (
    <div
      className={classNames(
        "flex grow flex-col overflow-hidden",
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

      <div className="flex grow p-2 overflow-hidden">
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
    <ul className="space-y-0.5 grow overflow-y-scroll">
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
          </li>
        );
      })}
    </ul>
  );
}

export interface Diagnostic {
  id: string;
  message: string;
  severity: Severity;
  range: Range | null;
  textRange: TextRange | null;
  raw: TyDiagnostic;
}

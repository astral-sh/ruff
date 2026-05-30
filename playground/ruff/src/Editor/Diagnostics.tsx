import { Diagnostic } from "ruff_wasm";
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
            {diagnostic.details.length > 0 ? (
              <ul className="pl-3 font-mono text-gray-500 whitespace-pre-wrap">
                {diagnostic.details.map((detail, index) => (
                  <li key={index}>
                    <Detail detail={detail} onGoTo={onGoTo} />
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

function Detail({
  detail,
  onGoTo,
}: {
  detail: Diagnostic["details"][number];
  onGoTo(line: number, column: number): void;
}) {
  const start = detail.start_location;
  const [prefix, message] = splitSubdiagnosticMessage(detail.message);

  if (start == null) {
    return <span>{detail.message}</span>;
  }

  return (
    <>
      {prefix}
      <button
        onClick={() => onGoTo(start.row, start.column)}
        className="text-start cursor-pointer text-current underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
      >
        {message}
        <span className="text-gray-500">
          {" "}
          [Ln {start.row}, Col {start.column}]
        </span>
      </button>
    </>
  );
}

function splitSubdiagnosticMessage(message: string): [string, string] {
  const separator = ": ";
  const separatorIndex = message.indexOf(separator);

  if (separatorIndex === -1) {
    return ["", message];
  }

  const prefixEnd = separatorIndex + separator.length;
  return [message.slice(0, prefixEnd), message.slice(prefixEnd)];
}

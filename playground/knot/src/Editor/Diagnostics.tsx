import { Diagnostic, Workspace } from "red_knot_wasm";
import classNames from "classnames";
import { Theme } from "shared";
import { useMemo } from "react";

interface Props {
  diagnostics: Diagnostic[];
  workspace: Workspace;
  theme: Theme;

  onGoTo(line: number, column: number): void;
}

export default function Diagnostics({
  diagnostics: unsorted,
  workspace,
  theme,
  onGoTo,
}: Props) {
  const diagnostics = useMemo(() => {
    const sorted = [...unsorted];
    sorted.sort((a, b) => {
      return (a.text_range()?.start ?? 0) - (b.text_range()?.start ?? 0);
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
        <Items
          diagnostics={diagnostics}
          onGoTo={onGoTo}
          workspace={workspace}
        />
      </div>
    </div>
  );
}

function Items({
  diagnostics,
  onGoTo,
  workspace,
}: {
  diagnostics: Array<Diagnostic>;
  workspace: Workspace;
  onGoTo(line: number, column: number): void;
}) {
  if (diagnostics.length === 0) {
    return (
      <div className={"flex flex-auto flex-col justify-center  items-center"}>
        Everything is looking good!
      </div>
    );
  }

  return (
    <ul className="space-y-0.5 grow overflow-y-scroll">
      {diagnostics.map((diagnostic, index) => {
        const position = diagnostic.to_range(workspace);
        const start = position?.start;
        const id = diagnostic.id();

        const startLine = (start?.line ?? 0) + 1;
        const startColumn = (start?.character ?? 0) + 1;

        return (
          <li key={`${diagnostic.text_range()?.start ?? 0}-${id ?? index}`}>
            <button
              onClick={() => onGoTo(startLine, startColumn)}
              className="w-full text-start cursor-pointer"
            >
              {diagnostic.message()}
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

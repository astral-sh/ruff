import type { IRange } from "monaco-editor";
import { type ReactNode, useCallback } from "react";

const URL_PATTERN = /\bhttps?:\/\/[^\s<>"']*[^\s<>"'.,;:!?)]/g;

export interface DiagnosticDetail {
  message: string;
  severity?: string;
  location: DiagnosticDetailLocation | null;
}

export interface DiagnosticDetailLocation extends IRange {
  path: string;
}

export function DiagnosticDetailItem({
  item,
  onGoTo,
}: {
  item: DiagnosticDetail;
  onGoTo?: (location: DiagnosticDetailLocation) => void;
}) {
  const severity = item.severity == null ? null : `${item.severity}: `;
  const location = item.location;
  const handleGoTo = useCallback(() => {
    if (location != null) {
      onGoTo?.(location);
    }
  }, [location, onGoTo]);

  if (location == null) {
    return (
      <span>
        {severity}
        <DiagnosticMessage>{item.message}</DiagnosticMessage>
      </span>
    );
  }

  const locationLabel = `[Ln ${location.startLineNumber}, Col ${location.startColumn}]`;

  // Keep the message outside the button so that only the bracketed source
  // location is presented as a navigation link. If the whole message is
  // hyperlinked, it becomes distracting.
  return (
    <>
      {severity}
      <DiagnosticMessage>{item.message}</DiagnosticMessage>{" "}
      {onGoTo == null ? (
        <span className="text-gray-500">{locationLabel}</span>
      ) : (
        <button
          type="button"
          onClick={handleGoTo}
          className="cursor-pointer text-gray-500 underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
        >
          {locationLabel}
        </button>
      )}
    </>
  );
}

function DiagnosticMessage({ children }: { children: string }) {
  const parts: ReactNode[] = [];
  let previousEnd = 0;

  for (const match of children.matchAll(URL_PATTERN)) {
    const start = match.index;
    const url = match[0];

    parts.push(children.slice(previousEnd, start));
    parts.push(
      <a
        key={start}
        href={url}
        target="_blank"
        rel="noreferrer"
        className="cursor-pointer text-gray-500 underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
      >
        {url}
      </a>,
    );
    previousEnd = start + url.length;
  }

  parts.push(children.slice(previousEnd));
  return parts;
}

import { useCallback } from "react";

export interface DiagnosticDetail<Location = DiagnosticDetailLocation> {
  message: string;
  severity?: string;
  location: Location | null;
}

export interface DiagnosticDetailLocation {
  line: number;
  column: number;
  displayPath?: string;
}

export function DiagnosticDetailItem<GoToLocation>({
  item,
  goToLocation,
  onGoTo,
}: {
  item: DiagnosticDetail;
  goToLocation?: GoToLocation | null;
  onGoTo?: (location: GoToLocation) => void;
}) {
  const severity = item.severity == null ? null : `${item.severity}: `;
  const location = item.location;
  const handleGoTo = useCallback(() => {
    if (goToLocation != null) {
      onGoTo?.(goToLocation);
    }
  }, [goToLocation, onGoTo]);

  if (location == null) {
    return (
      <span>
        {severity}
        {item.message}
      </span>
    );
  }

  const locationLabel =
    location.displayPath == null
      ? `[Ln ${location.line}, Col ${location.column}]`
      : `[${location.displayPath}: Ln ${location.line}, Col ${location.column}]`;

  // Keep the message outside the button so that only the bracketed source
  // location is presented as a navigation link. If the whole message is
  // hyperlinked, it becomes distracting.
  return (
    <>
      {severity}
      {item.message}{" "}
      {goToLocation == null || onGoTo == null ? (
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

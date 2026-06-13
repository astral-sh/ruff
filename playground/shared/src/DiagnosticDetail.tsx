export interface DiagnosticDetail {
  message: string;
  severity?: string;
  location?: DiagnosticDetailLocation;
}

export interface DiagnosticDetailLocation {
  line: number;
  column: number;
  displayPath?: string;
  onGoTo?: () => void;
}

export interface DiagnosticDetailInput<Location> {
  message: string;
  severity?: string;
  location: Location | null | undefined;
}

export function createDiagnosticDetail<Location>(
  item: DiagnosticDetailInput<Location>,
  createLocation: (location: Location) => DiagnosticDetailLocation,
): DiagnosticDetail {
  if (item.location == null) {
    return { message: item.message, severity: item.severity };
  }

  return {
    message: item.message,
    severity: item.severity,
    location: createLocation(item.location),
  };
}

export function DiagnosticDetailItem({ item }: { item: DiagnosticDetail }) {
  const severity = item.severity == null ? null : `${item.severity}: `;
  const location = item.location;

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
      {location.onGoTo == null ? (
        <span className="text-gray-500">{locationLabel}</span>
      ) : (
        <button
          type="button"
          onClick={location.onGoTo}
          className="cursor-pointer text-gray-500 underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
        >
          {locationLabel}
        </button>
      )}
    </>
  );
}

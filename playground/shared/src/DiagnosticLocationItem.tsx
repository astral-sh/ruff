export function DiagnosticLocationItem({
  prefix,
  message,
  locationLabel,
  onGoTo,
}: {
  prefix?: string;
  message: string;
  locationLabel?: string;
  onGoTo?: () => void;
}) {
  if (locationLabel == null) {
    return (
      <span>
        {prefix}
        {message}
      </span>
    );
  }

  const content = (
    <>
      {message}
      <span className="text-gray-500"> {locationLabel}</span>
    </>
  );

  return (
    <>
      {prefix}
      {onGoTo == null ? (
        <span>{content}</span>
      ) : (
        <button
          onClick={onGoTo}
          className="text-start cursor-pointer text-current underline decoration-dotted underline-offset-2 transition-colors hover:text-gray-400 dark:hover:text-gray-400"
        >
          {content}
        </button>
      )}
    </>
  );
}

export function isDiagnosticAnnotationMessage(
  message: string | null | undefined,
): message is string {
  return message != null && message.length > 0;
}

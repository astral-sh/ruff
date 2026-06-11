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

export function renderableSecondaryDiagnosticAnnotations<
  T extends {
    primary: boolean;
    message: string | null | undefined;
  },
>(annotations: readonly T[]): Array<T & { message: string }> {
  let seenPrimary = false;

  return annotations.filter(
    (annotation): annotation is T & { message: string } => {
      if (!seenPrimary && annotation.primary) {
        seenPrimary = true;
        return false;
      }

      // Match ty_server's LSP rendering: omit message-less secondary annotations.
      // Unlike subdiagnostics, these have no parent message to use as a fallback.
      // Ruff also represents highlight-only annotations with empty messages, and
      // Monaco related information requires a message.
      return annotation.message != null && annotation.message.length > 0;
    },
  );
}

export function secondaryAnnotationsWithMessages<
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

import { useEffect, useState } from "react";
import AstralButton from "./AstralButton";

type CopyMarkdownStatus = "initial" | "copying" | "copied";

export default function CopyMarkdownButton({
  onCopyMarkdown,
}: {
  onCopyMarkdown: () => Promise<void>;
}) {
  const [status, setStatus] = useState<CopyMarkdownStatus>("initial");

  useEffect(() => {
    if (status === "copied") {
      const timeout = setTimeout(() => setStatus("initial"), 2000);
      return () => clearTimeout(timeout);
    }
  }, [status]);

  return status === "copied" ? (
    <AstralButton
      type="button"
      className="relative flex-none leading-6 py-1.5 px-3 cursor-auto dark:shadow-copied text-xs"
    >
      <span
        className="absolute inset-0 flex items-center justify-center invisible"
        aria-hidden="true"
      >
        Copy Markdown
      </span>
      <span aria-hidden="false">Copied!</span>
    </AstralButton>
  ) : (
    <AstralButton
      type="button"
      className="relative flex-none leading-6 py-1.5 px-3 shadow-xs disabled:opacity-50 text-xs"
      disabled={status === "copying"}
      onClick={async () => {
        setStatus("copying");
        try {
          await onCopyMarkdown();
          setStatus("copied");
        } catch (error) {
          // eslint-disable-next-line no-console
          console.error("Failed to copy markdown", error);
          setStatus("initial");
        }
      }}
    >
      <span
        className="absolute inset-0 flex items-center justify-center"
        aria-hidden="false"
      >
        Copy Markdown
      </span>
      <span className="invisible" aria-hidden="true">
        Copied!
      </span>
    </AstralButton>
  );
}

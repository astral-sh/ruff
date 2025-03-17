import { useEffect, useState } from "react";
import AstralButton from "./AstralButton";

export default function ShareButton({ onShare }: { onShare?: () => void }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (copied) {
      const timeout = setTimeout(() => setCopied(false), 2000);
      return () => clearTimeout(timeout);
    }
  }, [copied]);

  return copied ? (
    <AstralButton
      type="button"
      className="relative flex-none leading-6 py-1.5 px-3 cursor-auto dark:shadow-copied"
    >
      <span
        className="absolute inset-0 flex items-center justify-center invisible"
        aria-hidden="true"
      >
        Share
      </span>
      <span aria-hidden="false">Copied!</span>
    </AstralButton>
  ) : (
    <AstralButton
      type="button"
      className="relative flex-none leading-6 py-1.5 px-3 shadow-xs disabled:opacity-50"
      disabled={!onShare || copied}
      onClick={
        onShare
          ? () => {
              setCopied(true);
              onShare();
            }
          : undefined
      }
    >
      <span
        className="absolute inset-0 flex items-center justify-center"
        aria-hidden="false"
      >
        Share
      </span>
      <span className="invisible" aria-hidden="true">
        Copied!
      </span>
    </AstralButton>
  );
}

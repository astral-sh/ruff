import { useEffect, useState } from "react";

export default function ShareButton({ onShare }: { onShare?: () => void }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (copied) {
      const timeout = setTimeout(() => setCopied(false), 2000);
      return () => clearTimeout(timeout);
    }
  }, [copied]);

  return copied ? (
    <button
      type="button"
      className="relative flex-none rounded-md text-sm font-semibold leading-6 py-1.5 px-3 cursor-auto text-ayu-accent shadow-copied dark:bg-ayu-accent/10"
    >
      <span
        className="absolute inset-0 flex items-center justify-center invisible"
        aria-hidden="true"
      >
        Share
      </span>
      <span className="" aria-hidden="false">
        Copied!
      </span>
    </button>
  ) : (
    <button
      type="button"
      className="relative flex-none rounded-md text-sm font-semibold leading-6 py-1.5 px-3 enabled:hover:bg-ayu-accent/70 bg-ayu-accent text-white shadow-sm dark:shadow-highlight/20 disabled:opacity-50"
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
    </button>
  );
}

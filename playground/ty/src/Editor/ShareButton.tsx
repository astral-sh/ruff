import { useCallback, useEffect, useRef, useState } from "react";
import { AstralButton } from "shared";

type ShareStatus = "initial" | "copying" | "copied";

export default function ShareButton({
  onShare,
  onCopyMarkdownLink,
  onCopyMarkdown,
}: {
  onShare: () => Promise<void>;
  onCopyMarkdownLink: () => Promise<void>;
  onCopyMarkdown: () => Promise<void>;
}) {
  const [status, setStatus] = useState<ShareStatus>("initial");
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (status === "copied") {
      const timeout = setTimeout(() => setStatus("initial"), 2000);
      return () => clearTimeout(timeout);
    }
  }, [status]);

  // Close on outside click or Escape
  useEffect(() => {
    if (!open) {
      return;
    }

    function handleClick(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setOpen(false);
      }
    }

    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const runAction = useCallback(
    async (action: () => Promise<void>) => {
      setOpen(false);
      setStatus("copying");
      try {
        await action();
        setStatus("copied");
      } catch (error) {
        // eslint-disable-next-line no-console
        console.error("Failed to share playground", error);
        setStatus("initial");
      }
    },
    [setOpen, setStatus],
  );

  return (
    <div ref={containerRef} className="relative">
      {status === "copied" ? (
        <AstralButton
          type="button"
          className="relative flex-none leading-6 py-1.5 px-3 cursor-auto dark:shadow-copied"
        >
          <span
            className="absolute inset-0 flex items-center justify-center invisible"
            aria-hidden="true"
          >
            Share ▾
          </span>
          <span aria-hidden="false">Copied!</span>
        </AstralButton>
      ) : (
        <AstralButton
          type="button"
          className="relative flex-none leading-6 py-1.5 px-3 shadow-xs disabled:opacity-50"
          disabled={status === "copying"}
          onClick={() => setOpen((prev) => !prev)}
        >
          Share ▾
        </AstralButton>
      )}

      {open && (
        <div
          className="
            absolute right-0 top-full mt-1 z-50
            rounded-md border shadow-lg
            border-gray-200 bg-white
            dark:border-gray-700 dark:bg-galaxy
          "
        >
          <button
            type="button"
            className="
              w-full text-left px-3 py-2 text-sm cursor-pointer whitespace-nowrap
              hover:bg-gray-100 dark:hover:bg-gray-800
              text-gray-900 dark:text-gray-100
              rounded-t-md
            "
            onClick={() => runAction(onShare)}
          >
            Copy link
          </button>
          <button
            type="button"
            className="
              w-full text-left px-3 py-2 text-sm cursor-pointer whitespace-nowrap
              hover:bg-gray-100 dark:hover:bg-gray-800
              text-gray-900 dark:text-gray-100
            "
            onClick={() => runAction(onCopyMarkdownLink)}
          >
            Copy link as Markdown
          </button>
          <button
            type="button"
            className="
              w-full text-left px-3 py-2 text-sm cursor-pointer whitespace-nowrap
              hover:bg-gray-100 dark:hover:bg-gray-800
              text-gray-900 dark:text-gray-100
              rounded-b-md
            "
            onClick={() => runAction(onCopyMarkdown)}
          >
            Copy link + code as Markdown
          </button>
        </div>
      )}
    </div>
  );
}

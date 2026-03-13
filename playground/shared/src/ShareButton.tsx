import classNames from "classnames";
import { startTransition, useActionState, useEffect } from "react";
import {
  Menu,
  MenuItem as AriaMenuItem,
  type MenuItemProps,
  MenuTrigger,
  Popover,
  Pressable,
  Separator,
} from "react-aria-components";
import AstralButton from "./AstralButton";

type ShareStatus = "initial" | "copied";
type ShareAction = "share" | "copyMarkdownLink" | "copyMarkdown" | "reset";

export default function ShareButton({
  onShare,
  onCopyMarkdownLink,
  onCopyMarkdown,
  onDownload,
}: {
  onShare: () => Promise<void>;
  onCopyMarkdownLink: () => Promise<void>;
  onCopyMarkdown: () => Promise<void>;
  onDownload(): void;
}) {
  const [status, dispatch, isPending] = useActionState(
    async (_previousStatus: ShareStatus, action: ShareAction) => {
      switch (action) {
        case "reset":
          return "initial";
        case "share":
          await onShare();
          break;
        case "copyMarkdownLink":
          await onCopyMarkdownLink();
          break;
        case "copyMarkdown":
          await onCopyMarkdown();
          break;
      }
      return "copied";
    },
    "initial",
  );

  useEffect(() => {
    if (status === "copied") {
      const timeout = setTimeout(
        () => startTransition(() => dispatch("reset")),
        2000,
      );
      return () => clearTimeout(timeout);
    }
  }, [status, dispatch]);

  const copied = status === "copied" && !isPending;

  return (
    <MenuTrigger>
      <Pressable>
        <AstralButton
          type="button"
          className={classNames(
            "relative flex-none leading-6 py-1.5 px-3",
            copied
              ? "cursor-auto dark:shadow-copied"
              : "shadow-xs disabled:opacity-50",
          )}
          disabled={isPending}
        >
          <span
            className={classNames(
              "absolute inset-0 flex items-center justify-center",
              copied && "invisible",
            )}
            aria-hidden={copied}
          >
            Share
          </span>
          <span
            className={classNames(!copied && "invisible")}
            aria-hidden={!copied}
          >
            Copied!
          </span>
        </AstralButton>
      </Pressable>
      <Popover className="min-w-[150px] bg-white dark:bg-galaxy border border-gray-200 dark:border-comet rounded-md shadow-lg mt-1 z-10">
        <Menu className="font-sans p-1 outline-0 max-h-[inherit] overflow-auto">
          <ShareMenuItem
            onAction={() => startTransition(() => dispatch("share"))}
          >
            Link
          </ShareMenuItem>
          <ShareMenuItem
            onAction={() => startTransition(() => dispatch("copyMarkdownLink"))}
          >
            Markdown Link
          </ShareMenuItem>
          <ShareMenuItem
            onAction={() => startTransition(() => dispatch("copyMarkdown"))}
          >
            Markdown
          </ShareMenuItem>
          <Separator className="my-1 border-t border-gray-200 dark:border-gray-700" />
          <ShareMenuItem onAction={onDownload}>Download ZIP</ShareMenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

function ShareMenuItem({ className, ...props }: MenuItemProps) {
  return (
    <AriaMenuItem
      className={classNames(
        "px-3 py-1.5 text-sm cursor-pointer outline-0 rounded",
        "text-galaxy dark:text-white",
        "hover:bg-gray-100 dark:hover:bg-space",
        className,
      )}
      {...props}
    />
  );
}

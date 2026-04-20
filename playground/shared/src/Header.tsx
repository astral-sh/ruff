import classNames from "classnames";
import RepoButton from "./RepoButton";
import ShareButton from "./ShareButton";
import ThemeButton from "./ThemeButton";
import { Theme } from "./theme";
import VersionTag from "./VersionTag";
import AstralButton from "./AstralButton";

export default function Header({
  theme,
  tool,
  version,
  onChangeTheme,
  onReset,
  edit,
  onShare,
  onCopyMarkdownLink,
  onCopyMarkdown,
  onDownload,
}: {
  theme: Theme;
  tool: "ruff" | "ty";
  version: string | null;
  onChangeTheme: (theme: Theme) => void;
  onReset?(): void;
  edit: number;
  onShare: () => Promise<void>;
  onCopyMarkdownLink: () => Promise<void>;
  onCopyMarkdown: () => Promise<void>;
  onDownload(): void;
}) {
  return (
    <div
      className="
        w-full
        flex
        justify-between
        antialiased
        border-b
        border-gray-200
        dark:border-b-radiate
        dark:bg-galaxy
      "
    >
      <div className="py-4 pl-2">
        <Logo name={tool} className="fill-galaxy dark:fill-radiate" />
      </div>
      <div className="flex items-center min-w-0 gap-4 mx-2">
        {version ? (
          <div className="hidden sm:flex">
            <VersionTag>{version}</VersionTag>
          </div>
        ) : null}
        <Divider />
        <RepoButton href={`https://github.com/astral-sh/${tool}`} />
        <Divider />
        <div className="max-sm:hidden flex">
          <ResetButton onClicked={onReset} />
        </div>
        <div className="max-sm:hidden flex">
          <ShareButton
            key={edit}
            onShare={onShare}
            onCopyMarkdownLink={onCopyMarkdownLink}
            onCopyMarkdown={onCopyMarkdown}
            onDownload={onDownload}
          />
        </div>
        <Divider />

        <ThemeButton theme={theme} onChange={onChangeTheme} />
      </div>
    </div>
  );
}

function Divider() {
  return (
    <div
      className={classNames(
        "max-sm:hidden",
        "visible",
        "w-px",
        "h-8",
        "bg-gray-200",
        "dark:bg-gray-700",
      )}
    />
  );
}

function Logo({ name, className }: { name: "ruff" | "ty"; className: string }) {
  switch (name) {
    case "ruff":
      return (
        <a href="https://docs.astral.sh/ruff">
          <svg
            height={32}
            viewBox="0 0 272 64"
            className={className}
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              fillRule="evenodd"
              clipRule="evenodd"
              d="M61.5 0C62.8807 0 64 1.11929 64 2.5V32.06C64 33.4407 62.8807 34.56 61.5 34.56H51.2V39.68H64V64H34.56V44.8H29.44V64H0V0H61.5ZM39.68 29.44V24.32H24.32V29.44H39.68ZM69.12 0H98.56V41.6H103.68V0H133.12V61.5C133.12 62.8807 132.001 64 130.62 64H71.62C70.2393 64 69.12 62.8807 69.12 61.5V0ZM202.24 0H145.86C144.479 0 143.36 1.11929 143.36 2.5V29.44H138.24V53.76H143.36V64H172.8V53.76H199.74C201.121 53.76 202.24 52.6407 202.24 51.26V29.44H172.8V24.32H202.24V0ZM214.98 0H271.36V24.32H241.92V29.44H271.36V51.26C271.36 52.6407 270.241 53.76 268.86 53.76H241.92V64H212.48V53.76H207.36V29.44H212.48V2.5C212.48 1.11929 213.599 0 214.98 0Z"
            />
          </svg>
        </a>
      );
    case "ty":
      return (
        <a href="https://docs.astral.sh/ty/">
          <svg
            height={32}
            viewBox="0 0 133 64"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            className={className}
          >
            <path
              d="M 64 10 H 37 V 0 H 5 V 10 H 0 V 35 H 5 V 54 C 5 59 10 64 16 64 H 64 V 40 H 37 V 35 H 54 C 59 35 64 30 64 24 V 10 Z M 99 10 H 104 V 0 H 133 V 54 C 133 59 128 64 122 64 h -53 V 40 H 101 V 35 H 79 C 74 35 69 30 69 24 V 0 H 99 Z"
            />
          </svg>
        </a>
      );
  }
}

function ResetButton({ onClicked }: { onClicked?: () => void }) {
  return (
    <AstralButton
      type="button"
      className="relative flex-none leading-6 py-1.5 px-3 shadow-xs disabled:opacity-50"
      disabled={onClicked == null}
      onClick={onClicked}
    >
      Reset
    </AstralButton>
  );
}

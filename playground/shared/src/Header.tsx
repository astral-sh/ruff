import classNames from "classnames";
import RepoButton from "./RepoButton";
import ThemeButton from "./ThemeButton";
import ShareButton from "./ShareButton";
import { Theme } from "./theme";
import VersionTag from "./VersionTag";
import AstralButton from "./AstralButton";

export default function Header({
  edit,
  theme,
  tool,
  version,
  onChangeTheme,
  onReset,
  onShare,
}: {
  edit: number | null;
  theme: Theme;
  tool: "ruff" | "ty";
  version: string | null;
  onChangeTheme: (theme: Theme) => void;
  onReset?(): void;
  onShare: () => void;
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
            <VersionTag>v{version}</VersionTag>
          </div>
        ) : null}
        <Divider />
        <RepoButton href={`https://github.com/astral-sh/${tool}`} />
        <Divider />
        <div className="max-sm:hidden flex">
          <ResetButton onClicked={onReset} />
        </div>
        <div className="max-sm:hidden flex">
          <ShareButton key={edit} onShare={onShare} />
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
      );
    case "ty":
      return (
        <svg
          height={32}
          viewBox="0 0 640 100"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
          className={className}
        >
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M431.998 9.98526C431.998 4.47055 436.469 0 441.984 0H522.013C527.528 0 531.998 4.47056 531.998 9.98526V100H485.998V70H477.998V100H431.998V9.98526ZM493.998 46V38H469.998V46H493.998Z"
          />
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M0 9.98526C0 4.47055 4.47055 0 9.98526 0H90.0147C95.5294 0 99.9999 4.47056 99.9999 9.98526V100H54V70H46V100H0V9.98526ZM62 46V38H38V46H62Z"
          />
          <path d="M107.998 9.98526C107.998 4.47055 112.469 0 117.983 0H198.013C203.527 0 207.998 4.47055 207.998 9.98526V30H161.998V22H153.998V38H198.013C203.527 38 207.998 42.4706 207.998 47.9853V90.0147C207.998 95.5294 203.527 100 198.013 100H117.983C112.469 100 107.998 95.5294 107.998 90.0147V70L153.998 70V78H161.998V62L117.983 62C112.469 62 107.998 57.5294 107.998 52.0147V9.98526Z" />
          <path d="M315.998 16H269.998V0H223.998V16H215.998V54H223.998V90.0147C223.998 95.5294 228.469 100 233.983 100H315.998V62H269.998V54H306.013C311.527 54 315.998 49.5294 315.998 44.0147V16Z" />
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M423.998 9.98526C423.998 4.47055 419.528 0 414.013 0H323.998V100H369.998V70H377.998V100H423.998V62H403.998V54H414.013C419.528 54 423.998 49.5294 423.998 44.0147V9.98526ZM385.998 38V46H361.998V38H385.998Z"
          />
          <path d="M585.999 62L639.998 62V100H539.999V2.18557e-06L585.999 0L585.999 62Z" />
        </svg>
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

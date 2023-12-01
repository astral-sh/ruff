import classNames from "classnames";
import RepoButton from "./RepoButton";
import ThemeButton from "./ThemeButton";
import ShareButton from "./ShareButton";
import { Theme } from "./theme";
import VersionTag from "./VersionTag";

export type Tab = "Source" | "Settings";

export default function Header({
  edit,
  theme,
  version,
  onChangeTheme,
  onShare,
}: {
  edit: number | null;
  theme: Theme;
  version: string | null;
  onChangeTheme: (theme: Theme) => void;
  onShare?: () => void;
}) {
  return (
    <div
      className={classNames(
        "w-full",
        "flex",
        "justify-between",
        "pl-5",
        "sm:pl-1",
        "pr-4",
        "lg:pr-6",
        "z-10",
        "top-0",
        "left-0",
        "-mb-px",
        "antialiased",
        "border-b",
        "border-gray-200",
        "dark:border-b-radiate",
        "dark:bg-galaxy",
      )}
    >
      <div className="py-4 pl-2">
        <svg
          width="136"
          height="32"
          viewBox="0 0 272 64"
          className="fill-galaxy dark:fill-radiate"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M61.5 0C62.8807 0 64 1.11929 64 2.5V32.06C64 33.4407 62.8807 34.56 61.5 34.56H51.2V39.68H64V64H34.56V44.8H29.44V64H0V0H61.5ZM39.68 29.44V24.32H24.32V29.44H39.68ZM69.12 0H98.56V41.6H103.68V0H133.12V61.5C133.12 62.8807 132.001 64 130.62 64H71.62C70.2393 64 69.12 62.8807 69.12 61.5V0ZM202.24 0H145.86C144.479 0 143.36 1.11929 143.36 2.5V29.44H138.24V53.76H143.36V64H172.8V53.76H199.74C201.121 53.76 202.24 52.6407 202.24 51.26V29.44H172.8V24.32H202.24V0ZM214.98 0H271.36V24.32H241.92V29.44H271.36V51.26C271.36 52.6407 270.241 53.76 268.86 53.76H241.92V64H212.48V53.76H207.36V29.44H212.48V2.5C212.48 1.11929 213.599 0 214.98 0Z"
          />
        </svg>
      </div>
      <div className="flex items-center min-w-0">
        {version ? (
          <div className="hidden sm:flex items-center">
            <VersionTag>v{version}</VersionTag>
          </div>
        ) : null}
        <Divider />
        <RepoButton />
        <Divider />
        <ShareButton key={edit} onShare={onShare} />
        <Divider />
        <ThemeButton theme={theme} onChange={onChangeTheme} />
      </div>
    </div>
  );
}

function Divider() {
  return (
    <div className="hidden sm:block mx-6 lg:mx-4 w-px h-8 bg-gray-200 dark:bg-gray-700" />
  );
}

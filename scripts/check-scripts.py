#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "pathspec>=1.1.1",
# ]
#
# [tool.ty.rules]
# truthiness-test-of-none-union = "warn"
# blanket-ignore-comment = "warn"
# missing-type-argument = "warn"
# possibly-unresolved-reference = "warn"
# unsound-return-statement = "warn"
# unsound-yield = "warn"
# unsupported-dynamic-base = "warn"
# division-by-zero = "warn"
# dynamic-function-decorator-return = "warn"
# unsound-assignment = "warn"
# redundant-condition-strict = "warn"
# disjoint-cast = "warn"
# missing-direct-dependency = "warn"
# ///

"""Update script lockfiles.

Run `uv run scripts/check-scripts.py` to update the lockfiles.
Use `--write-ty-args-file <path>` to write the discovered scripts to an argument file for ty
instead of updating lockfiles.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from collections.abc import Iterator
from pathlib import Path

from pathspec import GitIgnoreSpec

ROOT = Path(__file__).resolve().parent.parent
# Crate resources and completion fixtures are inputs, not project scripts.
COMPLETION_FIXTURES = Path("crates/ty_completion_eval/truth")


def scripts() -> Iterator[Path]:
    """Yield paths to Python scripts containing PEP 723 metadata.

    The paths are relative to the repository root. Files ignored by the root
    `.gitignore`, symlinks, crate resources, and completion fixtures are
    excluded. Scripts with invalid metadata are included so `uv lock` can
    report the error.
    """

    gitignore = GitIgnoreSpec.from_lines((ROOT / ".gitignore").read_text().splitlines())

    # uv's script listing skips invalid metadata, so find candidates before
    # asking uv to validate and lock them.
    #
    # We use `.walk()` here rather than `.rglob()` because some of our ignored directories are
    # huge; `.walk()` allows us to lazily inspect whether they're gitignored before recursively
    # descending into them.
    for directory, dirnames, filenames in ROOT.walk():
        dirnames[:] = [
            dirname
            for dirname in dirnames
            if (path := (directory / dirname).relative_to(ROOT)) != Path(".git")
            and not gitignore.match_file(f"{path.as_posix()}/")
        ]

        for filename in filenames:
            if not filename.endswith(".py"):
                continue

            full_path = directory / filename

            path = full_path.relative_to(ROOT)

            if path.is_relative_to(COMPLETION_FIXTURES):
                continue

            match full_path.relative_to(ROOT).parts:
                case ("crates", _, "resources", *_):
                    continue

            if (
                full_path.is_symlink()
                or not full_path.is_file()
                or gitignore.match_file(path)
            ):
                continue

            if b"# /// script" in full_path.read_bytes().splitlines():
                yield path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument(
        "--write-ty-args-file",
        type=Path,
        help="Write script paths to an argument file for ty",
    )
    args = parser.parse_args()

    if args.write_ty_args_file is not None:
        args.write_ty_args_file.write_text(
            "".join(f"{script}\n" for script in scripts()), encoding="utf-8"
        )
        return 0

    failed = False

    for script in scripts():
        print(script, flush=True)
        command = ["uv", "lock", "--script", str(script), "--refresh", "--no-locked"]
        result = subprocess.run(command, cwd=ROOT, check=False)
        failed |= result.returncode != 0

    return int(failed)


if __name__ == "__main__":
    sys.exit(main())

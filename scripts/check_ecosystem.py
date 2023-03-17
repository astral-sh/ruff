#!/usr/bin/env python3
"""Check two versions of ruff against a corpus of open-source code.

Example usage:

    scripts/check_ecosystem.py <path/to/ruff1> <path/to/ruff2>
"""

# ruff: noqa: T201

import argparse
import asyncio
import difflib
import heapq
import re
import tempfile
from asyncio.subprocess import PIPE, create_subprocess_exec
from contextlib import asynccontextmanager
from pathlib import Path
from typing import TYPE_CHECKING, NamedTuple, Self

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Iterator, Sequence


class Repository(NamedTuple):
    """A GitHub repository at a specific ref."""

    org: str
    repo: str
    ref: str
    select: str = "ALL"
    ignore: str = ""
    exclude: str = ""

    @asynccontextmanager
    async def clone(self: Self) -> "AsyncIterator[Path]":
        """Shallow clone this repository to a temporary directory."""
        with tempfile.TemporaryDirectory() as tmpdir:
            process = await create_subprocess_exec(
                "git",
                "clone",
                "--config",
                "advice.detachedHead=false",
                "--quiet",
                "--depth",
                "1",
                "--no-tags",
                "--branch",
                self.ref,
                f"https://github.com/{self.org}/{self.repo}",
                tmpdir,
            )

            await process.wait()

            yield Path(tmpdir)


REPOSITORIES = {
    "zulip": Repository("zulip", "zulip", "main"),
    "bokeh": Repository("bokeh", "bokeh", "branch-3.2"),
    "scikit-build": Repository("scikit-build", "scikit-build", "main"),
    "scikit-build-core": Repository("scikit-build", "scikit-build-core", "main"),
    "cibuildwheel": Repository("pypa", "cibuildwheel", "main"),
    "airflow": Repository("apache", "airflow", "main"),
    "typeshed": Repository("python", "typeshed", "main", select="PYI"),
}

SUMMARY_LINE_RE = re.compile(r"^(Found \d+ error.*)|(.*potentially fixable with.*)$")


class RuffError(Exception):
    """An error reported by ruff."""


async def check(
    *,
    ruff: Path,
    path: Path,
    select: str,
    ignore: str = "",
    exclude: str = "",
) -> "Sequence[str]":
    """Run the given ruff binary against the specified path."""
    ruff_args = ["check", "--no-cache", "--exit-zero", "--select", select]
    if ignore:
        ruff_args.extend(["--ignore", ignore])
    if exclude:
        ruff_args.extend(["--exclude", exclude])
    proc = await create_subprocess_exec(
        ruff.absolute(),
        *ruff_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )

    result, err = await proc.communicate()

    if proc.returncode != 0:
        raise RuffError(err.decode("utf8"))

    lines = [
        line
        for line in result.decode("utf8").splitlines()
        if not SUMMARY_LINE_RE.match(line)
    ]

    return sorted(lines)


class Diff(NamedTuple):
    """A diff between two runs of ruff."""

    removed: set[str]
    added: set[str]

    def __bool__(self: Self) -> bool:
        """Return true if this diff is non-empty."""
        return bool(self.removed or self.added)

    def __iter__(self: Self) -> "Iterator[str]":
        """Iterate through the changed lines in diff format."""
        for line in heapq.merge(sorted(self.removed), sorted(self.added)):
            if line in self.removed:
                yield f"- {line}"
            else:
                yield f"+ {line}"


async def compare(ruff1: Path, ruff2: Path, repo: Repository) -> Diff | None:
    """Check a specific repository against two versions of ruff."""
    removed, added = set(), set()

    async with repo.clone() as path:
        try:
            async with asyncio.TaskGroup() as tg:
                check1 = tg.create_task(
                    check(
                        ruff=ruff1,
                        path=path,
                        select=repo.select,
                        ignore=repo.ignore,
                        exclude=repo.exclude,
                    ),
                )
                check2 = tg.create_task(
                    check(
                        ruff=ruff2,
                        path=path,
                        select=repo.select,
                        ignore=repo.ignore,
                        exclude=repo.exclude,
                    ),
                )
        except ExceptionGroup as e:
            raise e.exceptions[0] from e

        for line in difflib.ndiff(check1.result(), check2.result()):
            if line.startswith("- "):
                removed.add(line[2:])
            elif line.startswith("+ "):
                added.add(line[2:])

    return Diff(removed, added)


async def main(*, ruff1: Path, ruff2: Path) -> None:
    """Check two versions of ruff against a corpus of open-source code."""
    results = await asyncio.gather(
        *[compare(ruff1, ruff2, repo) for repo in REPOSITORIES.values()],
        return_exceptions=True,
    )

    diffs = {name: result for name, result in zip(REPOSITORIES, results, strict=True)}

    total_removed = total_added = 0
    errors = 0

    for diff in diffs.values():
        if isinstance(diff, Exception):
            errors += 1
        else:
            total_removed += len(diff.removed)
            total_added += len(diff.added)

    if total_removed == 0 and total_added == 0 and errors == 0:
        print("\u2705 ecosystem check detected no changes.")
    else:
        changes = f"(+{total_added}, -{total_removed}, {errors} error(s))"

        print(f"\u2139\ufe0f ecosystem check **detected changes**. {changes}")
        print()

        for name, diff in diffs.items():
            if isinstance(diff, Exception):
                changes = "error"
                print(f"<details><summary>{name} ({changes})</summary>")
                print("<p>")
                print()

                print("```")
                print(str(diff))
                print("```")

                print()
                print("</p>")
                print("</details>")
            elif diff:
                changes = f"+{len(diff.added)}, -{len(diff.removed)}"
                print(f"<details><summary>{name} ({changes})</summary>")
                print("<p>")
                print()

                diff_str = "\n".join(diff)

                print("```diff")
                print(diff_str)
                print("```")

                print()
                print("</p>")
                print("</details>")
            else:
                continue


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Check two versions of ruff against a corpus of open-source code.",
        epilog="scripts/check_ecosystem.py <path/to/ruff1> <path/to/ruff2>",
    )

    parser.add_argument(
        "ruff1",
        type=Path,
    )
    parser.add_argument(
        "ruff2",
        type=Path,
    )

    args = parser.parse_args()

    asyncio.run(main(ruff1=args.ruff1, ruff2=args.ruff2))

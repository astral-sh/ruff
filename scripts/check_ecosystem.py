#!/usr/bin/env python3
"""Check two versions of ruff against a corpus of open-source code.

Example usage:

    scripts/check_ecosystem.py <path/to/ruff1> <path/to/ruff2>
"""

import argparse
import asyncio
import difflib
import heapq
import json
import logging
import re
import tempfile
from asyncio.subprocess import PIPE, create_subprocess_exec
from contextlib import asynccontextmanager
from pathlib import Path
from typing import TYPE_CHECKING, NamedTuple, Optional, Self

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Iterator, Sequence

logger = logging.getLogger(__name__)


class Repository(NamedTuple):
    """A GitHub repository at a specific ref."""

    org: str
    repo: str
    ref: Optional[str]
    select: str = ""
    ignore: str = ""
    exclude: str = ""

    @asynccontextmanager
    async def clone(self: Self) -> "AsyncIterator[Path]":
        """Shallow clone this repository to a temporary directory."""
        with tempfile.TemporaryDirectory() as tmpdir:
            logger.debug(f"Cloning {self.org}/{self.repo}")
            git_command = [
                "git",
                "clone",
                "--config",
                "advice.detachedHead=false",
                "--quiet",
                "--depth",
                "1",
                "--no-tags",
            ]
            if self.ref:
                git_command.extend(["--branch", self.ref])

            git_command.extend(
                [
                    f"https://github.com/{self.org}/{self.repo}",
                    tmpdir,
                ],
            )

            process = await create_subprocess_exec(*git_command)

            await process.wait()

            logger.debug(f"Finished cloning {self.org}/{self.repo}")

            yield Path(tmpdir)


REPOSITORIES = {
    "airflow": Repository("apache", "airflow", "main", select="ALL"),
    "bokeh": Repository("bokeh", "bokeh", "branch-3.2", select="ALL"),
    "build": Repository("pypa", "build", "main"),
    "cibuildwheel": Repository("pypa", "cibuildwheel", "main"),
    "disnake": Repository("DisnakeDev", "disnake", "master"),
    "scikit-build": Repository("scikit-build", "scikit-build", "main"),
    "scikit-build-core": Repository("scikit-build", "scikit-build-core", "main"),
    "typeshed": Repository("python", "typeshed", "main", select="PYI"),
    "zulip": Repository("zulip", "zulip", "main", select="ALL"),
}

SUMMARY_LINE_RE = re.compile(r"^(Found \d+ error.*)|(.*potentially fixable with.*)$")


class RuffError(Exception):
    """An error reported by ruff."""


async def check(
    *,
    ruff: Path,
    path: Path,
    name: str,
    select: str = "",
    ignore: str = "",
    exclude: str = "",
) -> "Sequence[str]":
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {ruff}")
    ruff_args = ["check", "--no-cache", "--exit-zero"]
    if select:
        ruff_args.extend(["--select", select])
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

    logger.debug(f"Finished checking {name} with {ruff}")

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
                        name=f"{repo.org}/{repo.repo}",
                        select=repo.select,
                        ignore=repo.ignore,
                        exclude=repo.exclude,
                    ),
                )
                check2 = tg.create_task(
                    check(
                        ruff=ruff2,
                        path=path,
                        name=f"{repo.org}/{repo.repo}",
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


def read_projects_jsonl(projects_jsonl: Path) -> dict[str, Repository]:
    """Read either of the two formats of https://github.com/akx/ruff-usage-aggregate."""
    repositories = {}
    for line in projects_jsonl.read_text().splitlines():
        data = json.loads(line)
        # Check the input format.
        if "items" in data:
            for item in data["items"]:
                # Pick only the easier case for now.
                if item["path"] != "pyproject.toml":
                    continue
                repository = item["repository"]
                assert re.fullmatch(r"[a-zA-Z0-9_.-]+", repository["name"]), repository[
                    "name"
                ]
                # GitHub doesn't give us any branch or pure rev info.  This would give
                # us the revision, but there's no way with git to just do
                # `git clone --depth 1` with a specific ref.
                # `ref = item["url"].split("?ref=")[1]` would be exact
                repositories[repository["name"]] = Repository(
                    repository["owner"]["login"],
                    repository["name"],
                    None,
                )
        else:
            assert "owner" in data, "Unknown ruff-usage-aggregate format"
            # Pick only the easier case for now.
            if data["path"] != "pyproject.toml":
                continue
            repositories[data["repo"]] = Repository(
                data["owner"],
                data["repo"],
                data.get("ref"),
            )
    return repositories


async def main(*, ruff1: Path, ruff2: Path, projects_jsonl: Optional[Path]) -> None:
    """Check two versions of ruff against a corpus of open-source code."""
    if projects_jsonl:
        repositories = read_projects_jsonl(projects_jsonl)
    else:
        repositories = REPOSITORIES

    logger.debug(f"Checking {len(repositories)} projects")

    results = await asyncio.gather(
        *[compare(ruff1, ruff2, repo) for repo in repositories.values()],
        return_exceptions=True,
    )

    diffs = dict(zip(repositories, results, strict=True))

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
        rule_changes: dict[str, tuple[int, int]] = {}
        changes = f"(+{total_added}, -{total_removed}, {errors} error(s))"

        print(f"\u2139\ufe0f ecosystem check **detected changes**. {changes}")
        print()

        for name, diff in diffs.items():
            if isinstance(diff, Exception):
                changes = "error"
                print(f"<details><summary>{name} ({changes})</summary>")
                repo = repositories[name]
                print(
                    f"https://github.com/{repo.org}/{repo.repo} ref {repo.ref} "
                    f"select {repo.select} ignore {repo.ignore} exclude {repo.exclude}",
                )
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

                # Count rule changes
                for line in diff_str.splitlines():
                    # Find rule change for current line or construction
                    # + <rule>/<path>:<line>:<column>: <rule_code> <message>
                    matches = re.findall(r": [A-Z]{1,3}[0-9]{3,4}", line)
                    if len(matches) == 0:
                        # Handle case where there are no regex matches e.g.
                        # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501, ERA001
                        # Which was found in local testing
                        continue

                    rule_code = matches[0][2:]  # Trim leading ": "

                    # Get current additions and removals for this rule
                    current_changes = rule_changes.get(rule_code, (0, 0))

                    # Check if addition or removal depending on the first character
                    if line[0] == "+":
                        current_changes = (current_changes[0] + 1, current_changes[1])
                    elif line[0] == "-":
                        current_changes = (current_changes[0], current_changes[1] + 1)

                    rule_changes[rule_code] = current_changes

            else:
                continue

        print(f"Rules changed: {len(rule_changes.keys())}")
        print()
        print("| Rule | Changes | Additions | Removals |")
        print("| ---- | ------- | --------- | -------- |")
        for rule, (additions, removals) in dict(
            sorted(
                rule_changes.items(),
                key=lambda x: (x[1][0] + x[1][1]),
                reverse=True,
            ),
        ).items():
            print(f"| {rule} | {additions + removals} | {additions} | {removals} |")

    logger.debug(f"Finished {len(repositories)} repositories")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Check two versions of ruff against a corpus of open-source code.",
        epilog="scripts/check_ecosystem.py <path/to/ruff1> <path/to/ruff2>",
    )

    parser.add_argument(
        "--projects",
        type=Path,
        help=(
            "Optional JSON files to use over the default repositories. "
            "Supports both github_search_*.jsonl and known-github-tomls.jsonl."
        ),
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Activate debug logging",
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

    if args.verbose:
        logging.basicConfig(level=logging.DEBUG)
    else:
        logging.basicConfig(level=logging.INFO)

    asyncio.run(main(ruff1=args.ruff1, ruff2=args.ruff2, projects_jsonl=args.projects))

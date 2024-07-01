#!/usr/bin/env python3
"""
**DEPRECATED** This script is being replaced by the ruff-ecosystem package.


Check two versions of ruff against a corpus of open-source code.

Example usage:

    scripts/check_ecosystem.py <path/to/ruff1> <path/to/ruff2>
"""

from __future__ import annotations

import argparse
import asyncio
import difflib
import heapq
import json
import logging
import re
import tempfile
import time
from asyncio.subprocess import PIPE, create_subprocess_exec
from contextlib import asynccontextmanager, nullcontext
from pathlib import Path
from signal import SIGINT, SIGTERM
from typing import TYPE_CHECKING, NamedTuple, Self, TypeVar

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Iterator, Sequence

logger = logging.getLogger(__name__)


class Repository(NamedTuple):
    """A GitHub repository at a specific ref."""

    org: str
    repo: str
    ref: str | None
    select: str = ""
    ignore: str = ""
    exclude: str = ""
    # Generating fixes is slow and verbose
    show_fixes: bool = False

    @asynccontextmanager
    async def clone(self: Self, checkout_dir: Path) -> AsyncIterator[Path]:
        """Shallow clone this repository to a temporary directory."""
        if checkout_dir.exists():
            logger.debug(f"Reusing {self.org}:{self.repo}")
            yield await self._get_commit(checkout_dir)
            return

        logger.debug(f"Cloning {self.org}:{self.repo}")
        git_clone_command = [
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
            git_clone_command.extend(["--branch", self.ref])

        git_clone_command.extend(
            [
                f"https://github.com/{self.org}/{self.repo}",
                checkout_dir,
            ],
        )

        git_clone_process = await create_subprocess_exec(
            *git_clone_command,
            env={"GIT_TERMINAL_PROMPT": "0"},
        )

        status_code = await git_clone_process.wait()

        logger.debug(
            f"Finished cloning {self.org}/{self.repo} with status {status_code}",
        )
        yield await self._get_commit(checkout_dir)

    def url_for(self: Self, commit_sha: str, path: str, lnum: int | None = None) -> str:
        """
        Return the GitHub URL for the given commit, path, and line number, if given.
        """
        # Default to main branch
        url = f"https://github.com/{self.org}/{self.repo}/blob/{commit_sha}/{path}"
        if lnum:
            url += f"#L{lnum}"
        return url

    async def _get_commit(self: Self, checkout_dir: Path) -> str:
        """Return the commit sha for the repository in the checkout directory."""
        git_sha_process = await create_subprocess_exec(
            *["git", "rev-parse", "HEAD"],
            cwd=checkout_dir,
            stdout=PIPE,
        )
        git_sha_stdout, _ = await git_sha_process.communicate()
        assert (
            await git_sha_process.wait() == 0
        ), f"Failed to retrieve commit sha at {checkout_dir}"
        return git_sha_stdout.decode().strip()


# Repositories to check
# We check most repositories with the default ruleset instead of all rules to avoid
# noisy reports when new rules are added; see https://github.com/astral-sh/ruff/pull/3590
REPOSITORIES: list[Repository] = [
    Repository("DisnakeDev", "disnake", "master"),
    Repository("PostHog", "HouseWatch", "main"),
    Repository("RasaHQ", "rasa", "main"),
    Repository("Snowflake-Labs", "snowcli", "main"),
    Repository("aiven", "aiven-client", "main"),
    Repository("alteryx", "featuretools", "main"),
    Repository("apache", "airflow", "main", select="ALL"),
    Repository("apache", "superset", "master", select="ALL"),
    Repository("aws", "aws-sam-cli", "develop"),
    Repository("binary-husky", "gpt_academic", "master"),
    Repository("bloomberg", "pytest-memray", "main"),
    Repository("bokeh", "bokeh", "branch-3.3", select="ALL"),
    # Disabled due to use of explicit `select` with `E999`, which is no longer
    # supported in `--preview`.
    # See: https://github.com/astral-sh/ruff/pull/12129
    # Repository("demisto", "content", "master"),
    Repository("docker", "docker-py", "main"),
    Repository("facebookresearch", "chameleon", "main"),
    Repository("freedomofpress", "securedrop", "develop"),
    Repository("fronzbot", "blinkpy", "dev"),
    Repository("ibis-project", "ibis", "master"),
    Repository("ing-bank", "probatus", "main"),
    Repository("jrnl-org", "jrnl", "develop"),
    Repository("langchain-ai", "langchain", "main"),
    Repository("latchbio", "latch", "main"),
    Repository("lnbits", "lnbits", "main"),
    Repository("milvus-io", "pymilvus", "master"),
    Repository("mlflow", "mlflow", "master"),
    Repository("model-bakers", "model_bakery", "main"),
    Repository("pandas-dev", "pandas", "main"),
    Repository("prefecthq", "prefect", "main"),
    Repository("pypa", "build", "main"),
    Repository("pypa", "cibuildwheel", "main"),
    Repository("pypa", "pip", "main"),
    Repository("pypa", "setuptools", "main"),
    Repository("python", "mypy", "master"),
    Repository("python", "typeshed", "main", select="PYI"),
    Repository("python-poetry", "poetry", "master"),
    Repository("qdrant", "qdrant-client", "master"),
    Repository("reflex-dev", "reflex", "main"),
    Repository("rotki", "rotki", "develop"),
    Repository("scikit-build", "scikit-build", "main"),
    Repository("scikit-build", "scikit-build-core", "main"),
    Repository("sphinx-doc", "sphinx", "master"),
    Repository("spruceid", "siwe-py", "main"),
    Repository("tiangolo", "fastapi", "master"),
    Repository("yandex", "ch-backup", "main"),
    Repository("zulip", "zulip", "main", select="ALL"),
]

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
    show_fixes: bool = False,
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {ruff}")
    ruff_args = ["check", "--no-cache", "--exit-zero"]
    if select:
        ruff_args.extend(["--select", select])
    if ignore:
        ruff_args.extend(["--ignore", ignore])
    if exclude:
        ruff_args.extend(["--exclude", exclude])
    if show_fixes:
        ruff_args.extend(["--show-fixes", "--ecosystem-ci"])

    start = time.time()
    proc = await create_subprocess_exec(
        ruff.absolute(),
        *ruff_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )
    result, err = await proc.communicate()
    end = time.time()

    logger.debug(f"Finished checking {name} with {ruff} in {end - start:.2f}")

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
    source_sha: str

    def __bool__(self: Self) -> bool:
        """Return true if this diff is non-empty."""
        return bool(self.removed or self.added)

    def __iter__(self: Self) -> Iterator[str]:
        """Iterate through the changed lines in diff format."""
        for line in heapq.merge(sorted(self.removed), sorted(self.added)):
            if line in self.removed:
                yield f"- {line}"
            else:
                yield f"+ {line}"


async def compare(
    ruff1: Path,
    ruff2: Path,
    repo: Repository,
    checkouts: Path | None = None,
) -> Diff | None:
    """Check a specific repository against two versions of ruff."""
    removed, added = set(), set()

    # By the default, the git clone are transient, but if the user provides a
    # directory for permanent storage we keep it there
    if checkouts:
        location_context = nullcontext(checkouts)
    else:
        location_context = tempfile.TemporaryDirectory()

    with location_context as checkout_parent:
        assert ":" not in repo.org
        assert ":" not in repo.repo
        checkout_dir = Path(checkout_parent).joinpath(f"{repo.org}:{repo.repo}")
        async with repo.clone(checkout_dir) as checkout_sha:
            try:
                async with asyncio.TaskGroup() as tg:
                    check1 = tg.create_task(
                        check(
                            ruff=ruff1,
                            path=checkout_dir,
                            name=f"{repo.org}/{repo.repo}",
                            select=repo.select,
                            ignore=repo.ignore,
                            exclude=repo.exclude,
                            show_fixes=repo.show_fixes,
                        ),
                    )
                    check2 = tg.create_task(
                        check(
                            ruff=ruff2,
                            path=checkout_dir,
                            name=f"{repo.org}/{repo.repo}",
                            select=repo.select,
                            ignore=repo.ignore,
                            exclude=repo.exclude,
                            show_fixes=repo.show_fixes,
                        ),
                    )
            except ExceptionGroup as e:
                raise e.exceptions[0] from e

            for line in difflib.ndiff(check1.result(), check2.result()):
                if line.startswith("- "):
                    removed.add(line[2:])
                elif line.startswith("+ "):
                    added.add(line[2:])

    return Diff(removed, added, checkout_sha)


def read_projects_jsonl(projects_jsonl: Path) -> dict[tuple[str, str], Repository]:
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
                repositories[(repository["owner"], repository["repo"])] = Repository(
                    repository["owner"]["login"],
                    repository["name"],
                    None,
                    select=repository.get("select"),
                    ignore=repository.get("ignore"),
                    exclude=repository.get("exclude"),
                )
        else:
            assert "owner" in data, "Unknown ruff-usage-aggregate format"
            # Pick only the easier case for now.
            if data["path"] != "pyproject.toml":
                continue
            repositories[(data["owner"], data["repo"])] = Repository(
                data["owner"],
                data["repo"],
                data.get("ref"),
                select=data.get("select"),
                ignore=data.get("ignore"),
                exclude=data.get("exclude"),
            )
    return repositories


DIFF_LINE_RE = re.compile(
    r"^(?P<pre>[+-]) (?P<inner>(?P<path>[^:]+):(?P<lnum>\d+):\d+:) (?P<post>.*)$",
)

T = TypeVar("T")


async def main(
    *,
    ruff1: Path,
    ruff2: Path,
    projects_jsonl: Path | None,
    checkouts: Path | None = None,
) -> None:
    """Check two versions of ruff against a corpus of open-source code."""
    if projects_jsonl:
        repositories = read_projects_jsonl(projects_jsonl)
    else:
        repositories = {(repo.org, repo.repo): repo for repo in REPOSITORIES}

    logger.debug(f"Checking {len(repositories)} projects")

    # https://stackoverflow.com/a/61478547/3549270
    # Otherwise doing 3k repositories can take >8GB RAM
    semaphore = asyncio.Semaphore(50)

    async def limited_parallelism(coroutine: T) -> T:
        async with semaphore:
            return await coroutine

    results = await asyncio.gather(
        *[
            limited_parallelism(compare(ruff1, ruff2, repo, checkouts))
            for repo in repositories.values()
        ],
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

        for (org, repo), diff in diffs.items():
            if isinstance(diff, Exception):
                changes = "error"
                print(f"<details><summary>{repo} ({changes})</summary>")
                repo = repositories[(org, repo)]
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
                print(f"<details><summary>{repo} ({changes})</summary>")
                print("<p>")
                print()

                repo = repositories[(org, repo)]
                diff_lines = list(diff)

                print("<pre>")
                for line in diff_lines:
                    match = DIFF_LINE_RE.match(line)
                    if match is None:
                        print(line)
                        continue

                    pre, inner, path, lnum, post = match.groups()
                    url = repo.url_for(diff.source_sha, path, int(lnum))
                    print(f"{pre} <a href='{url}'>{inner}</a> {post}")
                print("</pre>")

                print()
                print("</p>")
                print("</details>")

                # Count rule changes
                for line in diff_lines:
                    # Find rule change for current line or construction
                    # + <rule>/<path>:<line>:<column>: <rule_code> <message>
                    matches = re.search(r": ([A-Z]{1,4}[0-9]{3,4})", line)

                    if matches is None:
                        # Handle case where there are no regex matches e.g.
                        # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501
                        # Which was found in local testing
                        continue

                    rule_code = matches.group(1)

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

        if len(rule_changes.keys()) > 0:
            print(f"Rules changed: {len(rule_changes.keys())}")
            print()
            print("| Rule | Changes | Additions | Removals |")
            print("| ---- | ------- | --------- | -------- |")
            for rule, (additions, removals) in sorted(
                rule_changes.items(),
                key=lambda x: (x[1][0] + x[1][1]),
                reverse=True,
            ):
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
        "--checkouts",
        type=Path,
        help=(
            "Location for the git checkouts, in case you want to save them"
            " (defaults to temporary directory)"
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

    loop = asyncio.get_event_loop()
    if args.checkouts:
        args.checkouts.mkdir(exist_ok=True, parents=True)
    main_task = asyncio.ensure_future(
        main(
            ruff1=args.ruff1,
            ruff2=args.ruff2,
            projects_jsonl=args.projects,
            checkouts=args.checkouts,
        ),
    )
    # https://stackoverflow.com/a/58840987/3549270
    for signal in [SIGINT, SIGTERM]:
        loop.add_signal_handler(signal, main_task.cancel)
    try:
        loop.run_until_complete(main_task)
    finally:
        loop.close()

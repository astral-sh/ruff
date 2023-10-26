from __future__ import annotations

import re
import time
from asyncio import create_subprocess_exec
from dataclasses import dataclass
from pathlib import Path
from subprocess import PIPE
from typing import TYPE_CHECKING, Self, Sequence

from unidiff import PatchSet

from ruff_ecosystem import logger
from ruff_ecosystem.markdown import markdown_project_section
from ruff_ecosystem.types import Comparison, Diff, Result, RuffError

if TYPE_CHECKING:
    from ruff_ecosystem.projects import ClonedRepository, Project


FORMAT_IGNORE_LINES = re.compile("^warning: `ruff format` is a work-in-progress.*")


def summarize_format_result(result: Result) -> str:
    lines = []
    total_lines_removed = total_lines_added = 0
    total_files_modified = 0
    error_count = len(result.errored)
    patch_sets = []

    for project, comparison in result.completed:
        total_lines_added += comparison.diff.lines_added
        total_lines_removed += comparison.diff.lines_removed

        patch_set = PatchSet("\n".join(comparison.diff.lines))
        patch_sets.append(patch_set)
        total_files_modified += len(patch_set.modified_files)

    if total_lines_removed == 0 and total_lines_added == 0 and error_count == 0:
        return "\u2705 ecosystem check detected no format changes."

    # Summarize the total changes
    es = "s" if error_count != 1 else ""
    fs = "s" if total_files_modified != 1 else ""
    changes = f"(+{total_lines_added}, -{total_lines_removed} lines in {total_files_modified} file{fs}, {error_count} error{es})"
    lines.append(f"\u2139\ufe0f ecosystem check **detected format changes**. {changes}")
    lines.append("")

    # Then per-project changes
    for (project, comparison), patch_set in zip(result.completed, patch_sets):
        if not comparison.diff:
            continue  # Skip empty diffs

        files = len(patch_set.modified_files)
        s = "s" if files != 1 else ""
        title = f"+{comparison.diff.lines_added}, -{comparison.diff.lines_removed} lines in {files} file{s}"

        lines.extend(
            markdown_project_section(
                title=title,
                content=patch_set_with_permalinks(patch_set, comparison.repo),
                options=project.format_options,
                project=project,
            )
        )

    for project, error in result.errored:
        lines.extend(
            markdown_project_section(
                title="error",
                content=str(error),
                options=project.format_options,
                project=project,
            )
        )

    return "\n".join(lines)


async def ruff_format(
    *,
    executable: Path,
    path: Path,
    name: str,
    options: FormatOptions,
    diff: bool = False,
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Formatting {name} with {executable}")
    ruff_args = ["format"]

    if diff:
        ruff_args.append("--diff")

    start = time.time()
    proc = await create_subprocess_exec(
        executable.absolute(),
        *ruff_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )
    result, err = await proc.communicate()
    end = time.time()

    logger.debug(f"Finished formatting {name} with {executable} in {end - start:.2f}s")

    if proc.returncode not in [0, 1]:
        raise RuffError(err.decode("utf8"))

    lines = result.decode("utf8").splitlines()
    return lines


async def black_format(
    *,
    executable: Path,
    path: Path,
    name: str,
) -> Sequence[str]:
    """Run the given black binary against the specified path."""
    logger.debug(f"Formatting {name} with {executable}")
    black_args = []

    start = time.time()
    proc = await create_subprocess_exec(
        executable.absolute(),
        *black_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )
    result, err = await proc.communicate()
    end = time.time()

    logger.debug(f"Finished formatting {name} with {executable} in {end - start:.2f}s")

    if proc.returncode != 0:
        raise RuffError(err.decode("utf8"))

    lines = result.decode("utf8").splitlines()
    return [line for line in lines if not FORMAT_IGNORE_LINES.match(line)]


async def compare_format(
    ruff_baseline_executable: Path,
    ruff_comparison_executable: Path,
    options: FormatOptions,
    cloned_repo: ClonedRepository,
):
    # Run format without diff to get the baseline
    await ruff_format(
        executable=ruff_baseline_executable.resolve(),
        path=cloned_repo.path,
        name=cloned_repo.fullname,
        options=options,
    )
    # Then get the diff from stdout
    diff = await ruff_format(
        executable=ruff_comparison_executable.resolve(),
        path=cloned_repo.path,
        name=cloned_repo.fullname,
        options=options,
        diff=True,
    )

    return create_format_comparison(cloned_repo, FormatDiff(lines=diff))


def create_format_comparison(repo: ClonedRepository, diff: str) -> FormatComparison:
    return FormatComparison(diff=diff, repo=repo)


@dataclass(frozen=True)
class FormatOptions:
    """
    Ruff format options.
    """

    def to_cli_args(self) -> list[str]:
        args = ["format", "--diff"]
        return args


@dataclass(frozen=True)
class FormatDiff(Diff):
    """A diff from ruff format."""

    lines: list[str]

    def __bool__(self: Self) -> bool:
        """Return true if this diff is non-empty."""
        return bool(self.lines)

    @property
    def added(self) -> set[str]:
        return set(line for line in self.lines if line.startswith("+"))

    @property
    def removed(self) -> set[str]:
        return set(line for line in self.lines if line.startswith("-"))


@dataclass(frozen=True)
class FormatComparison(Comparison):
    diff: FormatDiff
    repo: ClonedRepository


@dataclass(frozen=True)
class FormatResult(Result):
    comparisons: tuple[Project, FormatComparison]


def patch_set_with_permalinks(patch_set: PatchSet, repo: ClonedRepository) -> str:
    lines = []
    for file_patch in patch_set:
        for hunk in file_patch:
            # Note: The line number is not exact because we formatted the repository for
            #        a baseline; we can't know the exact line number in the original
            #        source file.
            hunk_link = repo.url_for(file_patch.path, hunk.source_start)
            hunk_lines = str(hunk).splitlines()

            # Add a link before the hunk
            link_title = file_patch.path + "~L" + str(hunk.source_start)
            lines.append(f"<a href='{hunk_link}'>{link_title}</a>")

            # Wrap the contents of the hunk in a diff code block
            lines.append("```diff")
            lines.extend(hunk_lines[1:])
            lines.append("```")

    return "\n".join(lines)

from __future__ import annotations

import asyncio
import re
import time
from asyncio import create_subprocess_exec
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from subprocess import PIPE
from typing import TYPE_CHECKING, Iterator, Self, Sequence

from ruff_ecosystem import logger
from ruff_ecosystem.markdown import markdown_project_section, markdown_details
from ruff_ecosystem.types import (
    Comparison,
    Diff,
    Result,
    RuffError,
    Serializable,
)

if TYPE_CHECKING:
    from ruff_ecosystem.projects import ClonedRepository


# Matches lines that are summaries rather than diagnostics
CHECK_SUMMARY_LINE_RE = re.compile(r"^(Found \d+ error.*)|(.* fixable with .*)$")

# Parses a diagnostic line (in a diff) to retrieve path and line number
CHECK_DIFF_LINE_RE = re.compile(
    r"^(?P<pre>[+-]) (?P<inner>(?P<path>[^:]+):(?P<lnum>\d+):\d+:) (?P<post>.*)$",
)


async def compare_check(
    ruff_baseline_executable: Path,
    ruff_comparison_executable: Path,
    options: CheckOptions,
    cloned_repo: ClonedRepository,
) -> Comparison:
    async with asyncio.TaskGroup() as tg:
        baseline_task = tg.create_task(
            ruff_check(
                executable=ruff_baseline_executable.resolve(),
                path=cloned_repo.path,
                name=cloned_repo.fullname,
                options=options,
            ),
        )
        comparison_task = tg.create_task(
            ruff_check(
                executable=ruff_comparison_executable.resolve(),
                path=cloned_repo.path,
                name=cloned_repo.fullname,
                options=options,
            ),
        )

    baseline_output, comparison_output = (
        baseline_task.result(),
        comparison_task.result(),
    )
    diff = Diff.new(baseline_output, comparison_output)

    return Comparison(diff=diff, repo=cloned_repo)


def summarize_check_result(result: Result) -> str:
    # Calculate the total number of rule changes
    all_rule_changes = RuleChanges()
    for _, comparison in result.completed:
        all_rule_changes.update(RuleChanges.from_diff(comparison.diff))

    lines = []
    total_removed = all_rule_changes.total_removed()
    total_added = all_rule_changes.total_added()
    error_count = len(result.errored)

    if total_removed == 0 and total_added == 0 and error_count == 0:
        return "\u2705 ecosystem check detected no linter changes."

    # Summarize the total changes
    s = "s" if error_count != 1 else ""
    changes = f"(+{total_added}, -{total_removed}, {error_count} error{s})"

    lines.append(f"\u2139\ufe0f ecosystem check **detected linter changes**. {changes}")
    lines.append("")

    # Then per-project changes
    max_lines_per_project = 200
    for project, comparison in result.completed:
        if not comparison.diff:
            continue  # Skip empty diffs

        diff = deduplicate_and_sort_diff(comparison.diff)
        limited_diff = limit_rule_lines(diff, project.check_options.max_lines_per_rule)

        # Display the diff
        # Wrap with `<pre>` for code-styling with support for links
        diff_lines = ["<pre>"]
        for line in limited_diff[:max_lines_per_project]:
            diff_lines.append(add_permalink_to_diagnostic_line(comparison.repo, line))

        omitted_lines = len(limited_diff) - max_lines_per_project
        if omitted_lines > 0:
            diff_lines.append(f"... {omitted_lines} additional lines omitted")

        diff_lines.append("</pre>")

        lines.extend(
            markdown_project_section(
                title=f"+{diff.lines_added}, -{diff.lines_removed}",
                content=diff_lines,
                options=project.check_options.markdown(),
                project=project,
            )
        )

    for project, error in result.errored:
        lines.extend(
            markdown_project_section(
                title="error",
                content=str(error),
                options="",
                project=project,
            )
        )

    # Display a summary table of changed rules
    if all_rule_changes:
        table_lines = []
        table_lines.append("| Rule | Changes | Additions | Removals |")
        table_lines.append("| ---- | ------- | --------- | -------- |")
        for rule, total in sorted(
            all_rule_changes.total_changes_by_rule(),
            key=lambda item: item[1],  # Sort by the total changes
            reverse=True,
        ):
            additions, removals = (
                all_rule_changes.added[rule],
                all_rule_changes.removed[rule],
            )
            table_lines.append(f"| {rule} | {total} | {additions} | {removals} |")

        lines.extend(
            markdown_details(
                summary=f"Rules changed: {len(all_rule_changes.rule_codes())}",
                preface="",
                content=table_lines,
            )
        )

    return "\n".join(lines)


def add_permalink_to_diagnostic_line(repo: ClonedRepository, line: str) -> str:
    match = CHECK_DIFF_LINE_RE.match(line)
    if match is None:
        return line

    pre, inner, path, lnum, post = match.groups()
    url = repo.url_for(path, int(lnum))
    return f"{pre} <a href='{url}'>{inner}</a> {post}"


async def ruff_check(
    *, executable: Path, path: Path, name: str, options: CheckOptions
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {executable}")
    ruff_args = ["check", "--no-cache", "--exit-zero"]
    if options.select:
        ruff_args.extend(["--select", options.select])
    if options.ignore:
        ruff_args.extend(["--ignore", options.ignore])
    if options.exclude:
        ruff_args.extend(["--exclude", options.exclude])
    if options.show_fixes:
        ruff_args.extend(["--show-fixes", "--ecosystem-ci"])

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

    logger.debug(f"Finished checking {name} with {executable} in {end - start:.2f}s")

    if proc.returncode != 0:
        raise RuffError(err.decode("utf8"))

    # Strip summary lines so the diff is only diagnostic lines
    lines = [
        line
        for line in result.decode("utf8").splitlines()
        if not CHECK_SUMMARY_LINE_RE.match(line)
    ]

    return lines


@dataclass(frozen=True)
class CheckOptions(Serializable):
    """
    Ruff check options
    """

    select: str = ""
    ignore: str = ""
    exclude: str = ""

    # Generating fixes is slow and verbose
    show_fixes: bool = False

    # Limit the number of reported lines per rule
    max_lines_per_rule: int | None = 50

    def markdown(self) -> str:
        return f"select {self.select} ignore {self.ignore} exclude {self.exclude}"


@dataclass(frozen=True)
class RuleChanges:
    """
    The number of additions and removals by rule code
    """

    added: Counter = field(default_factory=Counter)
    removed: Counter = field(default_factory=Counter)

    def rule_codes(self) -> set[str]:
        return set(self.added.keys()).union(self.removed.keys())

    def __add__(self, other: Self) -> Self:
        if not isinstance(other, type(self)):
            return NotImplemented

        new = RuleChanges()
        new.update(self)
        new.update(other)
        return new

    def update(self, other: Self) -> Self:
        self.added.update(other.added)
        self.removed.update(other.removed)
        return self

    def total_added(self) -> int:
        return sum(self.added.values())

    def total_removed(self) -> int:
        return sum(self.removed.values())

    def total_changes_by_rule(self) -> Iterator[str, int]:
        """
        Yields the sum of changes for each rule
        """
        totals = Counter()
        totals.update(self.added)
        totals.update(self.removed)
        yield from totals.items()

    @classmethod
    def from_diff(cls: type[Self], diff: Diff) -> Self:
        """
        Parse a diff from `ruff check` to determine the additions and removals for each rule
        """
        rule_changes = cls()

        for line in sorted(diff.added):
            code = parse_rule_code(line)
            if code is not None:
                rule_changes.added[code] += 1

        for line in sorted(diff.removed):
            code = parse_rule_code(line)
            if code is not None:
                rule_changes.removed[code] += 1

        return rule_changes

    def __bool__(self):
        return bool(self.added or self.removed)


def parse_rule_code(line: str) -> str | None:
    """
    Parse the rule code from a diagnostic line

    + <rule>/<path>:<line>:<column>: <rule_code> <message>
    """
    matches = re.search(r": ([A-Z]{1,4}[0-9]{3,4})", line)

    if matches is None:
        # Handle case where there are no regex matches e.g.
        # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501, ERA001
        # Which was found in local testing
        return None

    return matches.group(1)


def deduplicate_and_sort_diff(diff: Diff) -> Diff:
    """
    Removes any duplicate lines and any unchanged lines from the diff.
    """
    lines = set()
    for line in diff:
        if line.startswith("+ "):
            lines.add(line)
        elif line.startswith("- "):
            lines.add(line)

    # Sort without the leading + or -
    return Diff(list(sorted(lines, key=lambda line: line[2:])))


def limit_rule_lines(diff: Diff, max_per_rule: int | None = 100) -> list[str]:
    """
    Reduce the diff to include a maximum number of lines for each rule.
    """
    if max_per_rule is None:
        return diff

    counts = Counter()
    reduced = []

    for line in diff:
        code = parse_rule_code(line)

        # Do not omit any unparsable lines
        if not code:
            reduced.append(line)
            continue

        counts[code] += 1
        if counts[code] > max_per_rule:
            continue

        reduced.append(line)

    # Add lines summarizing the omitted changes
    for code, count in counts.items():
        hidden_count = count - max_per_rule
        if hidden_count > 0:
            reduced.append(f"... {hidden_count} changes omitted for rule {code}")

    return reduced

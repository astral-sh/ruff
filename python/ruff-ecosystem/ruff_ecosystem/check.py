"""
Execution, comparison, and summary of `ruff check` ecosystem checks.
"""
from __future__ import annotations

import asyncio
import re
import time
from asyncio import create_subprocess_exec
from collections import Counter
import dataclasses
from dataclasses import dataclass, field
from pathlib import Path
from subprocess import PIPE
from typing import TYPE_CHECKING, Iterator, Self, Sequence

from ruff_ecosystem import logger
from ruff_ecosystem.markdown import markdown_details, markdown_project_section
from ruff_ecosystem.types import (
    Comparison,
    Diff,
    Result,
    RuffError,
    Serializable,
)

if TYPE_CHECKING:
    from ruff_ecosystem.projects import ClonedRepository, Project


# Matches lines that are summaries rather than diagnostics
CHECK_SUMMARY_LINE_RE = re.compile(r"^(Found \d+ error.*)|(.* fixable with .*)$")

# Parses a diagnostic line (in a diff) to retrieve path and line number
CHECK_DIFF_LINE_RE = re.compile(
    r"^(?P<pre>[+-]) (?P<inner>(?P<path>[^:]+):(?P<lnum>\d+):\d+:) (?P<post>.*)$",
)

CHECK_DIAGNOSTIC_LINE_RE = re.compile(
    r"^(?P<diff>[+-])? ?(?P<location>.*): (?P<code>[A-Z]{1,4}[0-9]{3,4})(?P<fixable> \[\*\])? (?P<message>.*)"
)

CHECK_VIOLATION_FIX_INDICATOR = " [*]"


def markdown_check_result(result: Result) -> str:
    # Calculate the total number of rule changes
    all_rule_changes = RuleChanges()
    project_rule_changes: dict[Project, Comparison] = {}
    for project, comparison in result.completed:
        project_rule_changes[project] = changes = RuleChanges.from_diff(comparison.diff)
        all_rule_changes.update(changes)

    lines = []
    total_removed = all_rule_changes.total_removed_violations()
    total_added = all_rule_changes.total_added_violations()
    total_added_fixes = all_rule_changes.total_added_fixes()
    total_removed_fixes = all_rule_changes.total_removed_fixes()
    error_count = len(result.errored)

    if total_removed == 0 and total_added == 0 and error_count == 0:
        return "\u2705 ecosystem check detected no linter changes."

    # Summarize the total changes
    s = "s" if error_count != 1 else ""
    changes = f"(+{total_added}, -{total_removed} violations; +{total_added_fixes}, -{total_removed_fixes} fixes; {error_count} error{s})"

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

        rule_changes = project_rule_changes[project]
        title = (
            f"+{rule_changes.added_violations}, "
            f"-{rule_changes.removed_violations} violations; "
            f"+{rule_changes.added_fixes} "
            f"-{rule_changes.removed_fixes} fixes; "
        )

        lines.extend(
            markdown_project_section(
                title=title,
                content=diff_lines,
                options=project.check_options,
                project=project,
            )
        )

    for project, error in result.errored:
        lines.extend(
            markdown_project_section(
                title="error",
                content=str(error),
                options=project.check_options,
                project=project,
            )
        )

    # Display a summary table of changed rules
    if all_rule_changes:
        table_lines = []
        table_lines.append("| code | total | + violation | - violation | + fix | - fix")
        table_lines.append("| ---- | ------- | --------- | -------- |")
        for rule, total in sorted(
            all_rule_changes.total_changes_by_rule(),
            key=lambda item: item[1],  # Sort by the total changes
            reverse=True,
        ):
            added_violations, removed_violations, added_fixes, removed_fixes = (
                all_rule_changes.added_violations[rule],
                all_rule_changes.removed_violations[rule],
                all_rule_changes.added_fixes[rule],
                all_rule_changes.removed_fixes[rule],
            )
            table_lines.append(
                f"| {rule} | {total} | {added_violations} | {removed_violations} "
                f"| {added_fixes} | {removed_fixes} |"
            )

        lines.extend(
            markdown_details(
                summary=f"Changes by rule ({len(all_rule_changes.rule_codes())} rules affected)",
                preface="",
                content=table_lines,
            )
        )

    return "\n".join(lines)


@dataclass(frozen=True)
class RuleChanges:
    """
    The number of additions and removals by rule code
    """

    added_violations: Counter = field(default_factory=Counter)
    removed_violations: Counter = field(default_factory=Counter)
    added_fixes: Counter = field(default_factory=Counter)
    removed_fixes: Counter = field(default_factory=Counter)

    def rule_codes(self) -> set[str]:
        return set(self.added_violations.keys()).union(self.removed_violations.keys())

    def __add__(self, other: Self) -> Self:
        if not isinstance(other, type(self)):
            return NotImplemented

        new = RuleChanges()
        new.update(self)
        new.update(other)
        return new

    def update(self, other: Self) -> Self:
        self.added_violations.update(other.added_violations)
        self.removed_violations.update(other.removed_violations)
        self.added_fixes.update(other.added_fixes)
        self.removed_fixes.update(other.removed_fixes)
        return self

    def total_added_violations(self) -> int:
        return sum(self.added_violations.values())

    def total_removed_violations(self) -> int:
        return sum(self.removed_violations.values())

    def total_added_fixes(self) -> int:
        return sum(self.added_fixes.values())

    def total_removed_fixes(self) -> int:
        return sum(self.removed_fixes.values())

    def total_changes_by_rule(self) -> Iterator[str, int]:
        """
        Yields the sum of changes for each rule
        """
        totals = Counter()
        totals.update(self.added_violations)
        totals.update(self.removed_violations)
        totals.update(self.added_fixes)
        totals.update(self.removed_fixes)
        yield from totals.items()

    @classmethod
    def from_diff(cls: type[Self], diff: Diff) -> Self:
        """
        Parse a diff from `ruff check` to determine the additions and removals for each rule
        """
        rule_changes = cls()

        parsed_lines: list[DiagnosticLine] = list(
            filter(None, (parse_diagnostic_line(line) for line in set(diff)))
        )
        added: set[DiagnosticLine] = set()
        removed: set[DiagnosticLine] = set()
        fix_only: set[DiagnosticLine] = set()

        for line in parsed_lines:
            if line.is_added:
                added.add(line.without_diff())
            elif line.is_removed:
                removed.add(line.without_diff())

        for line in parsed_lines:
            other_set = removed if line.is_added else added
            if (
                line.fix_available
                and line.without_fix_available().without_diff() in other_set
            ):
                if line.is_added:
                    rule_changes.added_fixes[line.rule_code] += 1
                else:
                    rule_changes.removed_fixes[line.rule_code] += 1
                fix_only.add(line)
            elif (
                not line.fix_available
                and line.with_fix_available().without_diff() in other_set
            ):
                if line.is_removed:
                    rule_changes.added_fixes[line.rule_code] += 1
                else:
                    rule_changes.removed_fixes[line.rule_code] += 1
                fix_only.add(line)

        for line in parsed_lines:
            if line in fix_only:
                continue

            if line.is_added:
                rule_changes.added_violations[line.rule_code] += 1
            elif line.is_removed:
                rule_changes.removed_violations[line.rule_code] += 1

        # only_fix_status_changed = set()

        # for (check, other) in [(added, removed), (removed, added)]
        #     for line in check:
        #         if line.fix_available:
        #             line_with_toggled = line.without_fix_available().construct_line()
        #         else:
        #             line_with_toggled = line.with_fix_available().construct_line()

        #         if line_without_fix in removed:
        #             rule_changes.added_fixes[line.rule_code] += 1

        #             # It's important to track both version of the line or we would falsely
        #             # report the paired `line_without_fix` as added or removed
        #             only_fix_status_changed.add(line)
        #             only_fix_status_changed.add(line_with_toggled)
        #         else:
        #             line_without_fix = line.without_fix_available().construct_line()
        #             if line_without_fix in removed:

        # for line in removed:
        #     line_without_fix = line.replace(CHECK_VIOLATION_FIX_INDICATOR, "")
        #     if line_without_fix in added:
        #         only_fix_status_changed.add(line)
        #         only_fix_status_changed.add(line_without_fix)

        # for line in added:
        #     code = parse_rule_code(line)
        #     if code is None:
        #         continue
        #     if line not in only_fix_status_changed:
        #         rule_changes.added_violations[code] += 1
        #     else:
        #         # We cannot know how many fixes are added or removed with the above approach
        #         # the values will always be symmetrical so we just report how many have changed
        #         rule_changes.added_fixes[code] += 1

        # for line in removed:
        #     code = parse_rule_code(line)
        #     if code is None:
        #         continue
        #     if line not in only_fix_status_changed:
        #         rule_changes.removed_violations[code] += 1

        return rule_changes

    def __bool__(self):
        return bool(self.added_violations or self.removed_violations)


@dataclass(frozen=True)
class DiagnosticLine:
    is_added: bool | None
    is_removed: bool | None
    fix_available: bool
    rule_code: str
    location: str
    message: str

    def construct_line(self) -> str:
        """
        Construct the line from the components
        """
        line = ""
        if self.is_added:
            line += "+ "
        elif self.is_removd:
            line += "- "
        line += f"{self.location}: {self.rule_code} "
        if self.fix_available:
            line += "[*] "
        line += self.message

    def with_fix_available(self) -> DiagnosticLine:
        return DiagnosticLine(**{**dataclasses.asdict(self), "fix_available": True})

    def without_fix_available(self) -> DiagnosticLine:
        return DiagnosticLine(**{**dataclasses.asdict(self), "fix_available": False})

    def without_diff(self) -> DiagnosticLine:
        return DiagnosticLine(
            **{**dataclasses.asdict(self), "is_added": None, "is_removed": None}
        )


def parse_diagnostic_line(line: str) -> DiagnosticLine | None:
    """
    Parse the rule code from a diagnostic line
    """
    match = CHECK_DIAGNOSTIC_LINE_RE.match(line)

    if match is None:
        # Handle case where there are no regex match e.g.
        # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501, ERA001
        # Which was found in local testing
        return None

    match_items = match.groupdict()

    return DiagnosticLine(
        location=match_items["location"],
        is_removed=match_items.get("diff") == "-",
        is_added=match_items.get("diff") == "+",
        fix_available=match_items.get("fixable") is not None,
        rule_code=match_items["code"],
        message=match_items["message"],
    )


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
        code = parse_diagnostic_line(line).rule_code

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


def add_permalink_to_diagnostic_line(repo: ClonedRepository, line: str) -> str:
    match = CHECK_DIFF_LINE_RE.match(line)
    if match is None:
        return line

    pre, inner, path, lnum, post = match.groups()
    url = repo.url_for(path, int(lnum))
    return f"{pre} <a href='{url}'>{inner}</a> {post}"


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

    # Drop unchanged lines from the diff since we don't need "context" for diagnostic lines
    diff = Diff.from_pair(baseline_output, comparison_output).without_unchanged_lines()

    return Comparison(diff=diff, repo=cloned_repo)


async def ruff_check(
    *, executable: Path, path: Path, name: str, options: CheckOptions
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {executable}")
    ruff_args = options.to_cli_args()

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

    def to_cli_args(self) -> list[str]:
        args = ["check", "--no-cache", "--exit-zero"]
        if self.select:
            args.extend(["--select", self.select])
        if self.ignore:
            args.extend(["--ignore", self.ignore])
        if self.exclude:
            args.extend(["--exclude", self.exclude])
        if self.show_fixes:
            args.extend(["--show-fixes", "--ecosystem-ci"])
        return args

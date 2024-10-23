"""
Execution, comparison, and summary of `ruff check` ecosystem checks.
"""

from __future__ import annotations

import asyncio
import dataclasses
import re
import time
from asyncio import create_subprocess_exec
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from subprocess import PIPE
from typing import TYPE_CHECKING, Iterable, Iterator, Self, Sequence

from ruff_ecosystem import logger
from ruff_ecosystem.markdown import (
    markdown_details,
    markdown_plus_minus,
    markdown_project_section,
)
from ruff_ecosystem.types import (
    Comparison,
    Diff,
    Result,
    ToolError,
)

if TYPE_CHECKING:
    from ruff_ecosystem.projects import (
        CheckOptions,
        ClonedRepository,
        ConfigOverrides,
        Project,
    )

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

GITHUB_MAX_COMMENT_LENGTH = 65536  # characters


def markdown_check_result(result: Result) -> str:
    """
    Render a `ruff check` ecosystem check result as markdown.
    """
    projects_with_changes = 0

    # Calculate the total number of rule changes
    all_rule_changes = RuleChanges()
    project_diffs = {
        project: CheckDiff.from_simple_diff(comparison.diff)
        for project, comparison in result.completed
    }
    project_rule_changes: dict[Project, RuleChanges] = {}
    for project, diff in project_diffs.items():
        project_rule_changes[project] = changes = RuleChanges.from_diff(diff)
        all_rule_changes.update(changes)

        if diff:
            projects_with_changes += 1

    lines: list[str] = []
    total_removed = all_rule_changes.total_removed_violations()
    total_added = all_rule_changes.total_added_violations()
    total_added_fixes = all_rule_changes.total_added_fixes()
    total_removed_fixes = all_rule_changes.total_removed_fixes()
    total_changes = (
        total_added + total_removed + total_added_fixes + total_removed_fixes
    )
    error_count = len(result.errored)
    total_affected_rules = len(all_rule_changes.rule_codes())

    if total_affected_rules == 0 and error_count == 0:
        return "\u2705 ecosystem check detected no linter changes."

    # Summarize the total changes
    if total_affected_rules == 0:
        # Only errors
        s = "s" if error_count != 1 else ""
        lines.append(
            f"\u2139\ufe0f ecosystem check **encountered linter errors**. (no lint changes; {error_count} project error{s})"
        )
    else:
        change_summary = (
            f"{markdown_plus_minus(total_added, total_removed)} violations, "
            f"{markdown_plus_minus(total_added_fixes, total_removed_fixes)} fixes "
            f"in {projects_with_changes} projects"
        )
        if error_count:
            s = "s" if error_count != 1 else ""
            change_summary += f"; {error_count} project error{s}"

        unchanged_projects = len(result.completed) - projects_with_changes
        if unchanged_projects:
            s = "s" if unchanged_projects != 1 else ""
            change_summary += f"; {unchanged_projects} project{s} unchanged"

        lines.append(
            f"\u2139\ufe0f ecosystem check **detected linter changes**. ({change_summary})"
        )
    lines.append("")

    # Display per project changes
    for project, comparison in result.completed:
        # TODO: This is not a performant way to check the length but the whole
        #       GitHub comment length issue needs to be addressed anyway
        if len(" ".join(lines)) > GITHUB_MAX_COMMENT_LENGTH // 3:
            lines.append("")
            lines.append(
                "_... Truncated remaining completed project reports due to GitHub comment length restrictions_"
            )
            lines.append("")
            break

        if not comparison.diff:
            continue  # Skip empty diffs

        diff = project_diffs[project]
        rule_changes = project_rule_changes[project]
        project_removed_violations = rule_changes.total_removed_violations()
        project_added_violations = rule_changes.total_added_violations()
        project_added_fixes = rule_changes.total_added_fixes()
        project_removed_fixes = rule_changes.total_removed_fixes()
        project_changes = (
            project_added_violations
            + project_removed_violations
            + project_added_fixes
            + project_removed_fixes
        )

        # Limit the number of items displayed per project to between 10 and 50
        # based on the proportion of total changes present in this project
        max_display_per_project = max(
            10,
            int(
                (
                    # TODO(zanieb): We take the `max` here to avoid division by zero errors where
                    # `total_changes` is zero but `total_affected_rules` is non-zero so we did not
                    # skip display. This shouldn't really happen and indicates a problem in the
                    # calculation of these values. Instead of skipping entirely when `total_changes`
                    # is zero, we'll attempt to report the results to help diagnose the problem.
                    #
                    # There's similar issues with the `max_display_per_rule` calculation immediately
                    # below as well.
                    project_changes / max(total_changes, 1)
                )
                * 50
            ),
        )

        # Limit the number of items displayed per rule to between 5 and the max for
        # the project based on the number of rules affected (less rules, more per rule)
        max_display_per_rule = max(
            5,
            # TODO: remove the need for the max() call here,
            # which is a workaround for if `len(rule_changes.rule_codes()) == 0`
            # (see comment in the assignment of `max_display_per_project` immediately above)
            max_display_per_project // max(len(rule_changes.rule_codes()), 1),
        )

        # Display the diff
        displayed_changes_per_rule = Counter()
        displayed_changes = 0

        # Wrap with `<pre>` for code-styling with support for links
        diff_lines = ["<pre>"]
        for line in diff.parsed_lines:
            rule_code = line.rule_code

            # Limit the number of changes we'll show per rule code
            if displayed_changes_per_rule[rule_code] > max_display_per_rule:
                continue

            diff_lines.append(
                add_permalink_to_diagnostic_line(comparison.repo, line.to_string())
            )

            displayed_changes_per_rule[rule_code] += 1
            displayed_changes += 1

            # If we just reached the maximum... display an omission line
            if displayed_changes_per_rule[rule_code] > max_display_per_rule:
                hidden_count = (
                    rule_changes.added_violations[rule_code]
                    + rule_changes.removed_violations[rule_code]
                    + rule_changes.added_fixes[rule_code]
                    + rule_changes.removed_fixes[rule_code]
                    - max_display_per_rule
                )
                diff_lines.append(
                    f"... {hidden_count} additional changes omitted for rule {rule_code}"
                )

            if displayed_changes >= max_display_per_project:
                break

        if project_changes > max_display_per_project:
            hidden_count = project_changes - displayed_changes
            diff_lines.append(
                f"... {hidden_count} additional changes omitted for project"
            )

        diff_lines.append("</pre>")

        title = (
            f"+{project_added_violations} "
            f"-{project_removed_violations} violations, "
            f"+{project_added_fixes} "
            f"-{project_removed_fixes} fixes"
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
                content=f"```\n{str(error).strip()}\n```",
                options=project.check_options,
                project=project,
            )
        )

    # Display a summary table of changed rules
    if all_rule_changes:
        table_lines = []
        table_lines.append(
            "| code | total | + violation | - violation | + fix | - fix |"
        )
        table_lines.append("| ---- | ------- | --------- | -------- | ----- | ---- |")
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
                summary=f"Changes by rule ({total_affected_rules} rules affected)",
                preface="",
                content=table_lines,
            )
        )

    return "\n".join(lines)


@dataclass(frozen=True)
class RuleChanges:
    """
    The number of additions and removals by rule code.

    While the attributes are frozen to avoid accidentally changing the value of an attribute,
    the counters themselves are mutable and this class can be mutated with `+` and `update`.
    """

    added_violations: Counter = field(default_factory=Counter)
    removed_violations: Counter = field(default_factory=Counter)
    added_fixes: Counter = field(default_factory=Counter)
    removed_fixes: Counter = field(default_factory=Counter)

    def rule_codes(self) -> set[str]:
        return (
            set(self.added_violations.keys())
            .union(self.removed_violations.keys())
            .union(self.added_fixes.keys())
            .union(self.removed_fixes.keys())
        )

    def __add__(self, other: Self) -> Self:
        if not isinstance(other, type(self)):
            return NotImplemented

        new = type(self)()
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

    def total_changes_by_rule(self) -> Iterator[tuple[str, int]]:
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
    def from_diff(cls: type[Self], diff: CheckDiff) -> Self:
        """
        Parse a diff from `ruff check` to determine the additions and removals for each rule
        """
        rule_changes = cls()

        for line in diff.parsed_lines:
            if line.is_added:
                if line in diff.fix_only_lines:
                    if line.fix_available:
                        rule_changes.added_fixes[line.rule_code] += 1
                    else:
                        rule_changes.removed_fixes[line.rule_code] += 1
                else:
                    rule_changes.added_violations[line.rule_code] += 1
            elif line.is_removed:
                if line in diff.fix_only_lines:
                    if line.fix_available:
                        rule_changes.removed_fixes[line.rule_code] += 1
                    else:
                        rule_changes.added_fixes[line.rule_code] += 1
                else:
                    rule_changes.removed_violations[line.rule_code] += 1

        return rule_changes

    def __bool__(self):
        return bool(
            self.added_violations
            or self.removed_violations
            or self.added_fixes
            or self.removed_fixes
        )


@dataclass(frozen=True)
class DiagnosticLine:
    is_added: bool | None
    is_removed: bool | None
    fix_available: bool
    rule_code: str
    location: str
    message: str

    def to_string(self) -> str:
        """
        Construct the line from the components
        """
        line = ""
        if self.is_added:
            line += "+ "
        elif self.is_removed:
            line += "- "
        line += f"{self.location}: {self.rule_code} "
        if self.fix_available:
            line += "[*] "
        line += self.message
        return line

    def with_fix_available(self) -> DiagnosticLine:
        return DiagnosticLine(**{**dataclasses.asdict(self), "fix_available": True})

    def without_fix_available(self) -> DiagnosticLine:
        return DiagnosticLine(**{**dataclasses.asdict(self), "fix_available": False})

    def without_diff(self) -> DiagnosticLine:
        return DiagnosticLine(
            **{**dataclasses.asdict(self), "is_added": None, "is_removed": None}
        )

    @classmethod
    def try_from_string(cls: type[Self], line: str) -> Self | None:
        """
        Parse the rule code from a diagnostic line string
        """
        match = CHECK_DIAGNOSTIC_LINE_RE.match(line)

        if match is None:
            # Handle case where there are no regex match e.g.
            # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501, ERA001
            # Which was found in local testing
            return None

        match_items = match.groupdict()

        return cls(
            location=match_items["location"],
            is_removed=match_items.get("diff") == "-",
            is_added=match_items.get("diff") == "+",
            fix_available=match_items.get("fixable") is not None,
            rule_code=match_items["code"],
            message=match_items["message"],
        )


class CheckDiff(Diff):
    """
    Extends the normal diff with diagnostic parsing
    """

    def __init__(
        self,
        lines: Iterable[str],
        parsed_lines: list[DiagnosticLine],
        fix_only_lines: set[DiagnosticLine],
    ) -> None:
        self.parsed_lines = parsed_lines
        self.fix_only_lines = fix_only_lines
        super().__init__(lines)

    @classmethod
    def from_simple_diff(cls, diff: Diff) -> CheckDiff:
        """
        Parse a simple diff to include check-specific analyses.
        """
        # Drop unchanged lines
        diff = diff.without_unchanged_lines()

        # Sort without account for the leading + / -
        sorted_lines = list(sorted(diff, key=lambda line: line[2:]))

        # Parse the lines, drop lines that cannot be parsed
        parsed_lines: list[DiagnosticLine] = list(
            filter(
                None,
                (DiagnosticLine.try_from_string(line) for line in sorted_lines),
            )
        )

        # Calculate which lines only changed fix availability
        fix_only: set[DiagnosticLine] = set()

        # TODO(zanieb): There has to be a cleaner way to express this logic
        # We check if each added line is available in the removed set with fix
        # availability toggled and vice-versa
        for line in parsed_lines:
            other_set = diff.removed if line.is_added else diff.added
            toggled = (
                line.without_fix_available()
                if line.fix_available
                else line.with_fix_available()
            )
            if toggled.without_diff().to_string() in other_set:
                fix_only.add(line)

        return CheckDiff(
            lines=sorted_lines, parsed_lines=parsed_lines, fix_only_lines=fix_only
        )


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
    config_overrides: ConfigOverrides,
    cloned_repo: ClonedRepository,
) -> Comparison:
    with config_overrides.patch_config(cloned_repo.path, options.preview):
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

    diff = Diff.from_pair(baseline_output, comparison_output)

    return Comparison(diff=diff, repo=cloned_repo)


async def ruff_check(
    *, executable: Path, path: Path, name: str, options: CheckOptions
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    ruff_args = options.to_ruff_args()
    logger.debug(f"Checking {name} with {executable} " + " ".join(ruff_args))

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
        raise ToolError(err.decode("utf8"))

    # Strip summary lines so the diff is only diagnostic lines
    lines = [
        line
        for line in result.decode("utf8").splitlines()
        if not CHECK_SUMMARY_LINE_RE.match(line)
    ]

    return lines

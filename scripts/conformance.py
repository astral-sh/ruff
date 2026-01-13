"""
Run typing conformance tests and compare results between two ty versions.

By default, this script will use `uv` to run the latest version of ty
as the new version with `uvx ty@latest`. This requires `uv` to be installed
and available in the system PATH.

If CONFORMANCE_SUITE_COMMIT is set, the hash will be used to create
links to the corresponding line in the conformance repository for each
diagnostic. Otherwise, it will default to `main'.

Examples:
    # Compare two specific ty versions
    %(prog)s --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7

    # Use local ty builds
    %(prog)s --old-ty ./target/debug/ty-old --new-ty ./target/debug/ty-new

    # Custom test directory
    %(prog)s --target-path custom/tests --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7

    # Show all diagnostics (not just changed ones)
    %(prog)s --all --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7

    # Show a diff with local paths to the test directory instead of table of links
    %(prog)s --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7 --format diff
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from enum import Flag, StrEnum, auto
from functools import reduce
from itertools import groupby
from operator import attrgetter, or_
from pathlib import Path
from textwrap import dedent
from typing import Any, Literal, Self

# The conformance tests include 4 types of errors:
# 1. Required errors (E): The type checker must raise an error on this line
# 2. Optional errors (E?): The type checker may raise an error on this line
# 3. Tagged errors (E[tag]): The type checker must raise at most one error on any of the lines in a file with matching tags
# 4. Tagged multi-errors (E[tag+]): The type checker should raise one or more errors on any of the tagged lines
# This regex pattern parses the error lines in the conformance tests, but the following
# implementation treats all errors as required errors.
CONFORMANCE_ERROR_PATTERN = re.compile(
    r"""
    \#\s*E                  # "# E" begins each error
    (?P<optional>\?)?       # Optional '?' (E?) indicates that an error is optional
    (?:                     # An optional tag for errors that may appear on multiple lines at most once
        \[
            (?P<tag>[^+\]]+)    # identifier
            (?P<multi>\+)?      # '+' indicates that an error may occur more than once on tagged lines
        \]
    )?
    (?:
        \s*:\s*(?P<description>.*) # optional description
    )?
    """,
    re.VERBOSE,
)

CONFORMANCE_URL = "https://github.com/python/typing/blob/{conformance_suite_commit}/conformance/tests/{filename}#L{line}"
CONFORMANCE_SUITE_COMMIT = os.environ.get("CONFORMANCE_SUITE_COMMIT", "main")


class Source(Flag):
    OLD = auto()
    NEW = auto()
    EXPECTED = auto()


class Classification(StrEnum):
    TRUE_POSITIVE = auto()
    FALSE_POSITIVE = auto()
    TRUE_NEGATIVE = auto()
    FALSE_NEGATIVE = auto()

    def into_title(self) -> str:
        match self:
            case Classification.TRUE_POSITIVE:
                return "True positives added 🎉"
            case Classification.FALSE_POSITIVE:
                return "False positives added 🫤"
            case Classification.TRUE_NEGATIVE:
                return "False positives removed 🎉"
            case Classification.FALSE_NEGATIVE:
                return "True positives removed 🫤"


@dataclass(kw_only=True, slots=True)
class Position:
    line: int
    column: int


@dataclass(kw_only=True, slots=True)
class Positions:
    begin: Position
    end: Position


@dataclass(kw_only=True, slots=True)
class Location:
    path: str
    positions: Positions

    def as_link(self) -> str:
        file = os.path.basename(self.path)
        link = CONFORMANCE_URL.format(
            conformance_suite_commit=CONFORMANCE_SUITE_COMMIT,
            filename=file,
            line=self.positions.begin.line,
        )
        return f"[{file}:{self.positions.begin.line}:{self.positions.begin.column}]({link})"


@dataclass(kw_only=True, slots=True)
class Diagnostic:
    check_name: str
    description: str
    severity: str
    fingerprint: str | None
    location: Location
    source: Source

    def __str__(self) -> str:
        return (
            f"{self.location.path}:{self.location.positions.begin.line}:"
            f"{self.location.positions.begin.column}: "
            f"{self.severity_for_display}[{self.check_name}] {self.description}"
        )

    @classmethod
    def from_gitlab_output(
        cls,
        dct: dict[str, Any],
        source: Source,
    ) -> Self:
        return cls(
            check_name=dct["check_name"],
            description=dct["description"],
            severity=dct["severity"],
            fingerprint=dct["fingerprint"],
            location=Location(
                path=dct["location"]["path"],
                positions=Positions(
                    begin=Position(
                        line=dct["location"]["positions"]["begin"]["line"],
                        column=dct["location"]["positions"]["begin"]["column"],
                    ),
                    end=Position(
                        line=dct["location"]["positions"]["end"]["line"],
                        column=dct["location"]["positions"]["end"]["column"],
                    ),
                ),
            ),
            source=source,
        )

    @property
    def key(self) -> str:
        """Key to group diagnostics by path and beginning line."""
        return f"{self.location.path}:{self.location.positions.begin.line}"

    @property
    def severity_for_display(self) -> str:
        return {
            "major": "error",
            "minor": "warning",
        }.get(self.severity, "unknown")


@dataclass(kw_only=True, slots=True)
class GroupedDiagnostics:
    key: str
    sources: Source
    old: Diagnostic | None
    new: Diagnostic | None
    expected: Diagnostic | None

    @property
    def changed(self) -> bool:
        return (Source.OLD in self.sources or Source.NEW in self.sources) and not (
            Source.OLD in self.sources and Source.NEW in self.sources
        )

    @property
    def classification(self) -> Classification:
        if Source.NEW in self.sources and Source.EXPECTED in self.sources:
            return Classification.TRUE_POSITIVE
        elif Source.NEW in self.sources and Source.EXPECTED not in self.sources:
            return Classification.FALSE_POSITIVE
        elif Source.EXPECTED in self.sources:
            return Classification.FALSE_NEGATIVE
        else:
            return Classification.TRUE_NEGATIVE

    def _render_row(self, diagnostic: Diagnostic):
        return f"| {diagnostic.location.as_link()} | {diagnostic.check_name} | {diagnostic.description} |"

    def _render_diff(self, diagnostic: Diagnostic, *, removed: bool = False):
        sign = "-" if removed else "+"
        return f"{sign} {diagnostic}"

    def display(self, format: Literal["diff", "github"]) -> str:
        match self.classification:
            case Classification.TRUE_POSITIVE | Classification.FALSE_POSITIVE:
                assert self.new is not None
                return (
                    self._render_diff(self.new)
                    if format == "diff"
                    else self._render_row(self.new)
                )

            case Classification.FALSE_NEGATIVE | Classification.TRUE_NEGATIVE:
                diagnostic = self.old or self.expected
                assert diagnostic is not None
                return (
                    self._render_diff(diagnostic, removed=True)
                    if format == "diff"
                    else self._render_row(diagnostic)
                )

            case _:
                raise ValueError(f"Unexpected classification: {self.classification}")


@dataclass(kw_only=True, slots=True)
class Statistics:
    true_positives: int = 0
    false_positives: int = 0
    false_negatives: int = 0

    @property
    def precision(self) -> float:
        if self.true_positives + self.false_positives > 0:
            return self.true_positives / (self.true_positives + self.false_positives)
        return 0.0

    @property
    def recall(self) -> float:
        if self.true_positives + self.false_negatives > 0:
            return self.true_positives / (self.true_positives + self.false_negatives)
        else:
            return 0.0

    @property
    def total(self) -> int:
        return self.true_positives + self.false_positives


def collect_expected_diagnostics(path: Path) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    for file in path.resolve().rglob("*.py"):
        for idx, line in enumerate(file.read_text().splitlines(), 1):
            if error := re.search(CONFORMANCE_ERROR_PATTERN, line):
                diagnostics.append(
                    Diagnostic(
                        check_name="conformance",
                        description=error.group("description")
                        or error.group("tag")
                        or "Missing",
                        severity="major",
                        fingerprint=None,
                        location=Location(
                            path=file.as_posix(),
                            positions=Positions(
                                begin=Position(
                                    line=idx,
                                    column=error.start(),
                                ),
                                end=Position(
                                    line=idx,
                                    column=error.end(),
                                ),
                            ),
                        ),
                        source=Source.EXPECTED,
                    )
                )

    assert diagnostics, "Failed to discover any expected diagnostics!"
    return diagnostics


def collect_ty_diagnostics(
    ty_path: list[str],
    source: Source,
    tests_path: str = ".",
    python_version: str = "3.12",
) -> list[Diagnostic]:
    process = subprocess.run(
        [
            *ty_path,
            "check",
            f"--python-version={python_version}",
            "--output-format=gitlab",
            "--exit-zero",
            tests_path,
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=15,
    )

    if process.returncode != 0:
        print(process.stderr)
        raise RuntimeError(f"ty check failed with exit code {process.returncode}")

    return [
        Diagnostic.from_gitlab_output(dct, source=source)
        for dct in json.loads(process.stdout)
    ]


def group_diagnostics_by_key(
    old: list[Diagnostic], new: list[Diagnostic], expected: list[Diagnostic]
) -> list[GroupedDiagnostics]:
    diagnostics = [
        *old,
        *new,
        *expected,
    ]
    sorted_diagnostics = sorted(diagnostics, key=attrgetter("key"))

    grouped = []
    for key, group in groupby(sorted_diagnostics, key=attrgetter("key")):
        group = list(group)
        sources: Source = reduce(or_, (diag.source for diag in group))
        grouped.append(
            GroupedDiagnostics(
                key=key,
                sources=sources,
                old=next(filter(lambda diag: diag.source == Source.OLD, group), None),
                new=next(filter(lambda diag: diag.source == Source.NEW, group), None),
                expected=next(
                    filter(lambda diag: diag.source == Source.EXPECTED, group), None
                ),
            )
        )

    return grouped


def compute_stats(
    grouped_diagnostics: list[GroupedDiagnostics], source: Source
) -> Statistics:
    if source == source.EXPECTED:
        # ty currently raises a false positive here due to incomplete enum.Flag support
        # see https://github.com/astral-sh/ty/issues/876
        num_errors = sum(
            1
            for g in grouped_diagnostics
            if source.EXPECTED in g.sources  # ty:ignore[unsupported-operator]
        )
        return Statistics(
            true_positives=num_errors, false_positives=0, false_negatives=0
        )

    def increment(statistics: Statistics, grouped: GroupedDiagnostics) -> Statistics:
        if (source in grouped.sources) and (Source.EXPECTED in grouped.sources):
            statistics.true_positives += 1
        elif source in grouped.sources:
            statistics.false_positives += 1
        elif Source.EXPECTED in grouped.sources:
            statistics.false_negatives += 1
        return statistics

    return reduce(increment, grouped_diagnostics, Statistics())


def render_grouped_diagnostics(
    grouped: list[GroupedDiagnostics],
    *,
    changed_only: bool = True,
    format: Literal["diff", "github"] = "diff",
) -> str:
    if changed_only:
        grouped = [diag for diag in grouped if diag.changed]

    sorted_by_class = sorted(
        grouped,
        key=attrgetter("classification"),
        reverse=True,
    )

    match format:
        case "diff":
            header = ["```diff"]
            footer = "```"
        case "github":
            header = [
                "| Location | Name | Message |",
                "|----------|------|---------|",
            ]
            footer = ""
        case _:
            raise ValueError("format must be one of 'diff' or 'github'")

    lines = []
    for classification, group in groupby(
        sorted_by_class, key=attrgetter("classification")
    ):
        group = list(group)

        lines.append(f"## {classification.into_title()}:")
        lines.append("")

        lines.extend(header)

        for diag in group:
            lines.append(diag.display(format=format))

        lines.append(footer)

    return "\n".join(lines)


def diff_format(
    diff: float,
    *,
    greater_is_better: bool = True,
    neutral: bool = False,
    is_percentage: bool = False,
):
    increased = diff > 0
    good = "(✅)" if not neutral else ""
    bad = "(❌)" if not neutral else ""
    up = "⏫"
    down = "⏬"

    match (greater_is_better, increased):
        case (True, True):
            return f"{up}{good}"
        case (False, True):
            return f"{up}{bad}"
        case (True, False):
            return f"{down}{bad}"
        case (False, False):
            return f"{down}{good}"


def render_summary(grouped_diagnostics: list[GroupedDiagnostics]):
    def format_metric(diff: float, old: float, new: float):
        if diff > 0:
            return f"increased from {old:.2%} to {new:.2%}"
        if diff < 0:
            return f"decreased from {old:.2%} to {new:.2%}"
        return f"held steady at {old:.2%}"

    old = compute_stats(grouped_diagnostics, source=Source.OLD)
    new = compute_stats(grouped_diagnostics, source=Source.NEW)

    precision_change = new.precision - old.precision
    recall_change = new.recall - old.recall
    true_pos_change = new.true_positives - old.true_positives
    false_pos_change = new.false_positives - old.false_positives
    false_neg_change = new.false_negatives - old.false_negatives
    total_change = new.total - old.total

    true_pos_diff = diff_format(true_pos_change, greater_is_better=True)
    false_pos_diff = diff_format(false_pos_change, greater_is_better=False)
    false_neg_diff = diff_format(false_neg_change, greater_is_better=False)
    precision_diff = diff_format(
        precision_change, greater_is_better=True, is_percentage=True
    )
    recall_diff = diff_format(recall_change, greater_is_better=True, is_percentage=True)
    total_diff = diff_format(total_change, neutral=True)

    table = dedent(
        f"""
        ## Typing Conformance

        ### Summary

        | Metric | Old | New | Diff | Outcome |
        |--------|-----|-----|------|---------|
        | True Positives  | {old.true_positives} | {new.true_positives} | {true_pos_change:+} | {true_pos_diff} |
        | False Positives | {old.false_positives} | {new.false_positives} | {false_pos_change:+} | {false_pos_diff} |
        | False Negatives | {old.false_negatives} | {new.false_negatives} | {false_neg_change:+} | {false_neg_diff} |
        | Total Diagnostics | {old.total} | {new.total} | {total_change} | {total_diff} |
        | Precision | {old.precision:.2%} | {new.precision:.2%} | {precision_change:+.2%} | {precision_diff} |
        | Recall | {old.recall:.2%} | {new.recall:.2%} | {recall_change:+.2%} | {recall_diff} |

        """
    )

    summary = (
        f"The percentage of diagnostics emitted that were expected errors"
        f" {format_metric(precision_change, old.precision, new.precision)},"
        " and the percentage of expected errors that received a diagnostic"
        f" {format_metric(recall_change, old.recall, new.recall)}."
    )

    return "\n".join([table, summary])


def parse_args():
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "--old-ty",
        nargs="+",
        help="Command to run old version of ty (default: uvx ty@0.0.1a35)",
    )

    parser.add_argument(
        "--new-ty",
        nargs="+",
        default=["uvx", "ty@latest"],
        help="Command to run new version of ty (default: uvx ty@0.0.7)",
    )

    parser.add_argument(
        "--tests-path",
        type=Path,
        default=Path("typing/conformance/tests"),
        help="Path to conformance tests directory (default: typing/conformance/tests)",
    )

    parser.add_argument(
        "--python-version",
        type=str,
        default="3.12",
        help="Python version to assume when running ty (default: 3.12)",
    )

    parser.add_argument(
        "--all",
        action="store_true",
        help="Show all diagnostics, not just changed ones",
    )

    parser.add_argument(
        "--format", type=str, choices=["diff", "github"], default="github"
    )

    parser.add_argument(
        "--output",
        type=Path,
        help="Write output to file instead of stdout",
    )

    args = parser.parse_args()

    if args.old_ty is None:
        raise ValueError("old_ty is required")

    return args


def main():
    args = parse_args()

    expected = collect_expected_diagnostics(args.tests_path)

    old = collect_ty_diagnostics(
        ty_path=args.old_ty,
        tests_path=str(args.tests_path),
        source=Source.OLD,
        python_version=args.python_version,
    )

    new = collect_ty_diagnostics(
        ty_path=args.new_ty,
        tests_path=str(args.tests_path),
        source=Source.NEW,
        python_version=args.python_version,
    )

    grouped = group_diagnostics_by_key(
        old=old,
        new=new,
        expected=expected,
    )

    rendered = "\n\n".join(
        [
            render_summary(grouped),
            render_grouped_diagnostics(
                grouped, changed_only=not args.all, format=args.format
            ),
        ]
    )

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
    else:
        print(rendered)


if __name__ == "__main__":
    main()

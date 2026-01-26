"""
Run typing conformance tests and compare results between two ty versions.

By default, this script will use `uv` to run the latest version of ty
as the new version with `uvx ty@latest`. This requires `uv` to be installed
and available in the system PATH.

If CONFORMANCE_SUITE_COMMIT is set, the hash will be used to create
links to the corresponding line in the conformance repository for each
diagnostic. Otherwise, it will default to `main'.

Examples:
    # Compare an older version of ty to latest
    %(prog)s --old-ty uvx ty@0.0.1a35

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
import tomllib
from collections.abc import Sequence, Set as AbstractSet
from dataclasses import dataclass
from enum import StrEnum, auto
from functools import reduce
from itertools import chain, groupby
from operator import attrgetter
from pathlib import Path
from textwrap import dedent
from typing import Any, Literal, Self, assert_never

# The conformance tests include 4 types of errors:
# 1. Required errors (E): The type checker must raise an error on this line
# 2. Optional errors (E?): The type checker may raise an error on this line
# 3. Tagged errors (E[tag]): The type checker must raise at most one error
#    on a set of lines with a matching tag
# 4. Tagged multi-errors (E[tag+]): The type checker should raise one or
#    more errors on a set of lines with a matching tag
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

CONFORMANCE_SUITE_COMMIT = os.environ.get("CONFORMANCE_SUITE_COMMIT", "main")
CONFORMANCE_DIR_WITH_README = (
    f"https://github.com/python/typing/blob/{CONFORMANCE_SUITE_COMMIT}/conformance/"
)
CONFORMANCE_URL = CONFORMANCE_DIR_WITH_README + "tests/{filename}#L{line}"


class Source(StrEnum):
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
                return "True positives added"
            case Classification.FALSE_POSITIVE:
                return "False positives added"
            case Classification.TRUE_NEGATIVE:
                return "False positives removed"
            case Classification.FALSE_NEGATIVE:
                return "True positives removed"


@dataclass(kw_only=True, slots=True)
class Evaluation:
    classification: Classification
    true_positives: int = 0
    false_positives: int = 0
    true_negatives: int = 0
    false_negatives: int = 0


class Change(StrEnum):
    ADDED = auto()
    REMOVED = auto()
    UNCHANGED = auto()

    def into_title(self) -> str:
        match self:
            case Change.ADDED:
                return "Optional Diagnostics Added"
            case Change.REMOVED:
                return "Optional Diagnostics Removed"
            case Change.UNCHANGED:
                return "Optional Diagnostics Unchanged"


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
    path: Path
    positions: Positions

    def as_link(self) -> str:
        file = self.path.name
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
    location: Location
    source: Source
    optional: bool
    # tag identifying an error that can occur on multiple lines
    tag: str | None
    # True if one or more errors can occur on lines with the same tag
    multi: bool

    def __post_init__(self, *args, **kwargs) -> None:
        # Remove check name prefix from description
        self.description = self.description.replace(f"{self.check_name}: ", "")
        # Escape pipe characters for GitHub markdown tables
        self.description = self.description.replace("|", "\\|")

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
            location=Location(
                path=Path(dct["location"]["path"]).resolve(),
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
            optional=False,
            tag=None,
            multi=False,
        )

    @property
    def key(self) -> str:
        """Key to group diagnostics by path and beginning line or path and tag."""
        return (
            f"{self.location.path.as_posix()}:{self.location.positions.begin.line}"
            if self.tag is None
            else f"{self.location.path.as_posix()}:{self.tag}"
        )

    @property
    def severity_for_display(self) -> str:
        return {
            "major": "error",
            "minor": "warning",
        }.get(self.severity, "unknown")


@dataclass(kw_only=True, slots=True)
class GroupedDiagnostics:
    key: str
    sources: AbstractSet[Source]
    old: list[Diagnostic]
    new: list[Diagnostic]
    expected: list[Diagnostic]

    @property
    def change(self) -> Change:
        if Source.NEW in self.sources and Source.OLD not in self.sources:
            return Change.ADDED
        elif Source.OLD in self.sources and Source.NEW not in self.sources:
            return Change.REMOVED
        else:
            return Change.UNCHANGED

    @property
    def optional(self) -> bool:
        return bool(self.expected) and all(
            diagnostic.optional for diagnostic in self.expected
        )

    @property
    def multi(self) -> bool:
        return bool(self.expected) and all(
            diagnostic.multi for diagnostic in self.expected
        )

    def diagnostics_by_source(self, source: Source) -> list[Diagnostic]:
        match source:
            case Source.NEW:
                return self.new
            case Source.OLD:
                return self.old
            case Source.EXPECTED:
                return self.expected

    def classify(self, source: Source) -> Evaluation:
        diagnostics = self.diagnostics_by_source(source)

        if source in self.sources:
            if self.optional:
                return Evaluation(
                    classification=Classification.TRUE_POSITIVE,
                    true_positives=len(diagnostics),
                    false_positives=0,
                    true_negatives=0,
                    false_negatives=0,
                )

            if Source.EXPECTED in self.sources:
                distinct_lines = len(
                    {
                        diagnostic.location.positions.begin.line
                        for diagnostic in diagnostics
                    }
                )
                expected_max = len(self.expected) if self.multi else 1

                if 1 <= distinct_lines <= expected_max:
                    return Evaluation(
                        classification=Classification.TRUE_POSITIVE,
                        true_positives=len(diagnostics),
                        false_positives=0,
                        true_negatives=0,
                        false_negatives=0,
                    )
                else:
                    # We select the line with the most diagnostics
                    # as our true positive, while the rest are false positives
                    max_line = max(
                        groupby(
                            diagnostics, key=lambda d: d.location.positions.begin.line
                        ),
                        key=lambda x: len(x[1]),
                    )
                    remaining = len(diagnostics) - max_line
                    # We can never exceed the number of distinct lines
                    # if the diagnostic is multi, so we ignore that case
                    return Evaluation(
                        classification=Classification.FALSE_POSITIVE,
                        true_positives=max_line,
                        false_positives=remaining,
                        true_negatives=0,
                        false_negatives=0,
                    )
            else:
                return Evaluation(
                    classification=Classification.FALSE_POSITIVE,
                    true_positives=0,
                    false_positives=len(diagnostics),
                    true_negatives=0,
                    false_negatives=0,
                )

        elif Source.EXPECTED in self.sources:
            if self.optional:
                return Evaluation(
                    classification=Classification.TRUE_NEGATIVE,
                    true_positives=0,
                    false_positives=0,
                    true_negatives=len(diagnostics),
                    false_negatives=0,
                )
            return Evaluation(
                classification=Classification.FALSE_NEGATIVE,
                true_positives=0,
                false_positives=0,
                true_negatives=0,
                false_negatives=1,
            )

        else:
            return Evaluation(
                classification=Classification.TRUE_NEGATIVE,
                true_positives=0,
                false_positives=0,
                true_negatives=1,
                false_negatives=0,
            )

    def _render_row(self, diagnostics: list[Diagnostic]):
        locs = []
        check_names = []
        descriptions = []

        for diagnostic in diagnostics:
            loc = (
                diagnostic.location.as_link()
                if diagnostic.location
                else f"`{diagnostic.tag}`"
            )
            locs.append(loc)
            check_names.append(diagnostic.check_name)
            descriptions.append(diagnostic.description)

        return f"| {'<br>'.join(locs)} | {'<br>'.join(check_names)} | {'<br>'.join(descriptions)} |"

    def _render_diff(self, diagnostics: list[Diagnostic], *, removed: bool = False):
        sign = "-" if removed else "+"
        return "\n".join(f"{sign} {diagnostic}" for diagnostic in diagnostics)

    def display(self, format: Literal["diff", "github"]) -> str:
        eval = self.classify(Source.NEW)
        match eval.classification:
            case Classification.TRUE_POSITIVE | Classification.FALSE_POSITIVE:
                assert self.new is not None
                return (
                    self._render_diff(self.new)
                    if format == "diff"
                    else self._render_row(self.new)
                )

            case Classification.FALSE_NEGATIVE | Classification.TRUE_NEGATIVE:
                diagnostics = self.old if self.old else self.expected

                return (
                    self._render_diff(diagnostics, removed=True)
                    if format == "diff"
                    else self._render_row(diagnostics)
                )


@dataclass(kw_only=True, slots=True)
class Statistics:
    true_positives: int = 0
    false_positives: int = 0
    false_negatives: int = 0
    total_diagnostics: int = 0

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


def collect_expected_diagnostics(test_files: Sequence[Path]) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    for file in test_files:
        for idx, line in enumerate(file.read_text().splitlines(), 1):
            if error := re.search(CONFORMANCE_ERROR_PATTERN, line):
                diagnostics.append(
                    Diagnostic(
                        check_name="conformance",
                        description=(error.group("description") or "Missing"),
                        severity="major",
                        location=Location(
                            path=file,
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
                        optional=error.group("optional") is not None,
                        tag=(
                            f"{file.name}:{error.group('tag')}"
                            if error.group("tag")
                            else None
                        ),
                        multi=error.group("multi") is not None,
                    )
                )

    assert diagnostics, "Failed to discover any expected diagnostics!"
    return diagnostics


def collect_ty_diagnostics(
    ty_path: list[str],
    source: Source,
    test_files: Sequence[Path],
    python_version: str = "3.12",
) -> list[Diagnostic]:
    process = subprocess.run(
        [
            *ty_path,
            "check",
            f"--python-version={python_version}",
            "--output-format=gitlab",
            "--ignore=assert-type-unspellable-subtype",
            "--exit-zero",
            *map(str, test_files),
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
        if dct["severity"] == "major"
    ]


def group_diagnostics_by_key(
    old: list[Diagnostic],
    new: list[Diagnostic],
    expected: list[Diagnostic],
) -> list[GroupedDiagnostics]:
    # propagate tags from expected diagnostics to old and new diagnostics
    tagged_lines = {
        (d.location.path.name, d.location.positions.begin.line): d.tag
        for d in expected
        if d.tag is not None
    }

    for diag in chain(old, new):
        diag.tag = tagged_lines.get(
            (diag.location.path.name, diag.location.positions.begin.line), None
        )

    diagnostics = [
        *old,
        *new,
        *expected,
    ]

    diagnostics = sorted(diagnostics, key=attrgetter("key"))
    grouped_diagnostics = []
    for key, group in groupby(diagnostics, key=attrgetter("key")):
        old_diagnostics: list[Diagnostic] = []
        new_diagnostics: list[Diagnostic] = []
        expected_diagnostics: list[Diagnostic] = []
        sources: set[Source] = set()

        for diag in group:
            sources.add(diag.source)
            match diag.source:
                case Source.OLD:
                    old_diagnostics.append(diag)
                case Source.NEW:
                    new_diagnostics.append(diag)
                case Source.EXPECTED:
                    expected_diagnostics.append(diag)

        grouped = GroupedDiagnostics(
            key=key,
            sources=sources,
            old=old_diagnostics,
            new=new_diagnostics,
            expected=expected_diagnostics,
        )
        grouped_diagnostics.append(grouped)

    return grouped_diagnostics


def compute_stats(
    grouped_diagnostics: list[GroupedDiagnostics],
    ty_version: Literal["new", "old"],
) -> Statistics:
    source = Source.NEW if ty_version == "new" else Source.OLD

    def increment(statistics: Statistics, grouped: GroupedDiagnostics) -> Statistics:
        eval = grouped.classify(source)
        statistics.true_positives += eval.true_positives
        statistics.false_positives += eval.false_positives
        statistics.false_negatives += eval.false_negatives
        statistics.total_diagnostics += len(grouped.diagnostics_by_source(source))
        return statistics

    return reduce(increment, grouped_diagnostics, Statistics())


def render_grouped_diagnostics(
    grouped: list[GroupedDiagnostics],
    *,
    changed_only: bool = True,
    format: Literal["diff", "github"] = "diff",
) -> str:
    if changed_only:
        grouped = [
            diag for diag in grouped if diag.change in (Change.ADDED, Change.REMOVED)
        ]

    get_change = attrgetter("change")

    def get_classification(diag) -> Classification:
        return diag.classify(Source.NEW).classification

    optional_diagnostics = sorted(
        (diag for diag in grouped if diag.optional),
        key=get_change,
        reverse=True,
    )
    required_diagnostics = sorted(
        (diag for diag in grouped if not diag.optional),
        key=get_classification,
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
    for group, diagnostics in chain(
        groupby(required_diagnostics, key=get_classification),
        groupby(optional_diagnostics, key=get_change),
    ):
        lines.append(f"### {group.into_title()}")
        lines.extend(["", "<details>", ""])

        lines.extend(header)

        for diag in diagnostics:
            lines.append(diag.display(format=format))

        lines.append(footer)
        lines.extend(["", "</details>", ""])

    return "\n".join(lines)


def diff_format(
    diff: float,
    *,
    greater_is_better: bool = True,
    neutral: bool = False,
) -> str:
    if diff == 0:
        return ""

    increased = diff > 0
    good = " (✅)" if not neutral else ""
    bad = " (❌)" if not neutral else ""
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
        case _:
            # The ty false positive seems to be due to insufficient type narrowing for tuples;
            # possibly related to https://github.com/astral-sh/ty/issues/493 and/or
            # https://github.com/astral-sh/ty/issues/887
            assert_never((greater_is_better, increased))  # ty: ignore[type-assertion-failure]


def render_summary(
    grouped_diagnostics: list[GroupedDiagnostics], *, force_summary_table: bool
) -> str:
    def format_metric(diff: float, old: float, new: float):
        if diff > 0:
            return f"increased from {old:.2%} to {new:.2%}"
        if diff < 0:
            return f"decreased from {old:.2%} to {new:.2%}"
        return f"held steady at {old:.2%}"

    old = compute_stats(grouped_diagnostics, ty_version="old")
    new = compute_stats(grouped_diagnostics, ty_version="new")

    assert new.true_positives > 0, (
        "Expected ty to have at least one true positive.\n"
        f"Sample of grouped diagnostics: {grouped_diagnostics[:5]}"
    )

    precision_change = new.precision - old.precision
    recall_change = new.recall - old.recall
    true_pos_change = new.true_positives - old.true_positives
    false_pos_change = new.false_positives - old.false_positives
    false_neg_change = new.false_negatives - old.false_negatives
    total_change = new.total_diagnostics - old.total_diagnostics

    base_header = f"[Typing conformance results]({CONFORMANCE_DIR_WITH_README})"

    if not force_summary_table and all(
        diag.change is Change.UNCHANGED for diag in grouped_diagnostics
    ):
        return dedent(
            f"""
            ## {base_header}

            No changes detected ✅
            """
        )

    true_pos_diff = diff_format(true_pos_change, greater_is_better=True)
    false_pos_diff = diff_format(false_pos_change, greater_is_better=False)
    false_neg_diff = diff_format(false_neg_change, greater_is_better=False)
    precision_diff = diff_format(precision_change, greater_is_better=True)
    recall_diff = diff_format(recall_change, greater_is_better=True)
    total_diff = diff_format(total_change, neutral=True)

    if (precision_change > 0 and recall_change >= 0) or (
        recall_change > 0 and precision_change >= 0
    ):
        header = f"{base_header} improved 🎉"
    elif (precision_change < 0 and recall_change <= 0) or (
        recall_change < 0 and precision_change <= 0
    ):
        header = f"{base_header} regressed ❌"
    else:
        header = base_header

    summary_paragraph = (
        f"The percentage of diagnostics emitted that were expected errors "
        f"{format_metric(precision_change, old.precision, new.precision)}. "
        f"The percentage of expected errors that received a diagnostic "
        f"{format_metric(recall_change, old.recall, new.recall)}."
    )

    return dedent(
        f"""
        ## {header}

        {summary_paragraph}

        ### Summary

        | Metric | Old | New | Diff | Outcome |
        |--------|-----|-----|------|---------|
        | True Positives  | {old.true_positives} | {new.true_positives} | {true_pos_change:+} | {true_pos_diff} |
        | False Positives | {old.false_positives} | {new.false_positives} | {false_pos_change:+} | {false_pos_diff} |
        | False Negatives | {old.false_negatives} | {new.false_negatives} | {false_neg_change:+} | {false_neg_diff} |
        | Total Diagnostics | {old.total_diagnostics} | {new.total_diagnostics} | {total_change:+} | {total_diff} |
        | Precision | {old.precision:.2%} | {new.precision:.2%} | {precision_change:+.2%} | {precision_diff} |
        | Recall | {old.recall:.2%} | {new.recall:.2%} | {recall_change:+.2%} | {recall_diff} |

        """
    )


def get_test_groups(root_dir: Path) -> AbstractSet[str]:
    """Adapted from typing/conformance/test_groups.py."""
    # Read the TOML file that defines the test groups. Each test
    # group has a name that associated test cases must start with.
    test_group_file = root_dir / "src" / "test_groups.toml"
    with open(test_group_file, "rb") as f:
        return tomllib.load(f).keys()


def get_test_cases(
    test_group_names: AbstractSet[str], tests_dir: Path
) -> Sequence[Path]:
    """Adapted from typing/conformance/test_groups.py."""
    # Filter test cases based on test group names. Files that do
    # not begin with a known test group name are assumed to be
    # files that support one or more tests.
    return [
        p
        for p in chain(tests_dir.glob("*.py"), tests_dir.glob("*.pyi"))
        if p.name.split("_")[0] in test_group_names
    ]


def parse_args():
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "--old-ty",
        nargs="+",
        help="Command to run old version of ty",
        required=True,
    )

    parser.add_argument(
        "--new-ty",
        nargs="+",
        default=["uvx", "ty@latest"],
        help="Command to run new version of ty (default: uvx ty@latest)",
    )

    parser.add_argument(
        "--tests-path",
        type=Path,
        default=Path("typing/conformance"),
        help="Path to conformance tests directory (default: typing/conformance)",
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

    parser.add_argument(
        "--force-summary-table",
        action="store_true",
        help="Always print the summary table, even if no changes were detected",
    )

    args = parser.parse_args()

    return args


def main():
    args = parse_args()
    tests_dir = args.tests_path.resolve().absolute()
    test_groups = get_test_groups(tests_dir)
    test_files = get_test_cases(test_groups, tests_dir / "tests")

    expected = collect_expected_diagnostics(test_files)

    old = collect_ty_diagnostics(
        ty_path=args.old_ty,
        test_files=test_files,
        source=Source.OLD,
        python_version=args.python_version,
    )

    new = collect_ty_diagnostics(
        ty_path=args.new_ty,
        test_files=test_files,
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
            render_summary(grouped, force_summary_table=args.force_summary_table),
            render_grouped_diagnostics(
                grouped, changed_only=not args.all, format=args.format
            ),
        ]
    )

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
        print(rendered, file=sys.stderr)
    else:
        print(rendered)


if __name__ == "__main__":
    main()

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

# Priority order for section headings: improvements first, regressions last.
TITLE_PRIORITY: dict[str, int] = {
    "True positives added": 0,
    "False positives removed": 1,
    "True positives changed": 2,
    "False positives changed": 3,
    "False positives added": 4,
    "True positives removed": 5,
    "Optional Diagnostics Added": 6,
    "Optional Diagnostics Removed": 7,
    "Optional Diagnostics Changed": 8,
}


class Source(StrEnum):
    OLD = auto()
    NEW = auto()
    EXPECTED = auto()


class Classification(StrEnum):
    TRUE_POSITIVE = auto()
    FALSE_POSITIVE = auto()
    TRUE_NEGATIVE = auto()
    FALSE_NEGATIVE = auto()

    def into_title(self, *, verb: Literal["added", "removed", "changed"]) -> str:
        match self:
            case Classification.TRUE_POSITIVE:
                return f"True positives {verb}"
            case Classification.FALSE_POSITIVE:
                return f"False positives {verb}"
            case Classification.TRUE_NEGATIVE:
                return f"True negatives {verb}"
            case Classification.FALSE_NEGATIVE:
                return f"False negatives {verb}"


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
    CHANGED = auto()
    UNCHANGED = auto()

    def into_title(self) -> str:
        match self:
            case Change.ADDED:
                return "Optional Diagnostics Added"
            case Change.REMOVED:
                return "Optional Diagnostics Removed"
            case Change.CHANGED:
                return "Optional Diagnostics Changed"
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
class TestCase:
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
        elif (
            Source.NEW in self.sources
            and Source.OLD in self.sources
            and not self.diagnostics_are_equivalent(self.old, self.new)
        ):
            return Change.CHANGED
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

    @staticmethod
    def diagnostics_are_equivalent(a: list[Diagnostic], b: list[Diagnostic]) -> bool:
        """Compare two diagnostic lists for equality, ignoring the `source` field."""

        def fingerprint(d: Diagnostic) -> tuple:
            return (
                d.check_name,
                d.description,
                d.severity,
                str(d.location.path),
                d.location.positions.begin.line,
                d.location.positions.begin.column,
            )

        return sorted(map(fingerprint, a)) == sorted(map(fingerprint, b))

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
                    # TODO: The ty diagnostics below are because we are not
                    # inferring a precise type for the `key` lambda, which gives
                    # us the wrong type for `max_line`.
                    # https://github.com/astral-sh/ty/issues/181
                    max_line = max(
                        groupby(
                            diagnostics, key=lambda d: d.location.positions.begin.line
                        ),
                        key=lambda x: len(x[1]),
                    )
                    remaining = len(diagnostics) - max_line  # ty: ignore[unsupported-operator]
                    # We can never exceed the number of distinct lines
                    # if the diagnostic is multi, so we ignore that case
                    return Evaluation(
                        classification=Classification.FALSE_POSITIVE,
                        true_positives=max_line,  # ty: ignore[invalid-argument-type]
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

    def _render_changed_row(
        self, diagnostics: list[Diagnostic], *, removed: bool
    ) -> str:
        """Like _render_row but prepends a sign column for use in "changed" sections."""
        sign = "-" if removed else "+"
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
        return f"| {sign} | {'<br>'.join(locs)} | {'<br>'.join(check_names)} | {'<br>'.join(descriptions)} |"


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
    extra_search_paths: Sequence[Path] = (),
) -> list[Diagnostic]:
    extra_search_path_args = [
        f"--extra-search-path={path}" for path in extra_search_paths
    ]
    process = subprocess.run(
        [
            *ty_path,
            "check",
            f"--python-version={python_version}",
            "--output-format=gitlab",
            "--ignore=assert-type-unspellable-subtype",
            "--error=invalid-legacy-positional-parameter",
            "--error=deprecated",
            "--error=redundant-final-classvar",
            "--exit-zero",
            *extra_search_path_args,
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
) -> list[TestCase]:
    # propagate tags from expected diagnostics to old and new diagnostics
    tagged_lines = {
        (d.location.path.name, d.location.positions.begin.line): d.tag
        for d in expected
        if d.tag is not None
    }

    for diag in chain(old, new):
        diag.tag = tagged_lines.get(
            (
                diag.location.path.name,
                diag.location.positions.begin.line,
            )
        )

    diagnostics = [
        *old,
        *new,
        *expected,
    ]

    diagnostics = sorted(diagnostics, key=attrgetter("key"))
    test_cases = []
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

        grouped = TestCase(
            key=key,
            sources=sources,
            old=old_diagnostics,
            new=new_diagnostics,
            expected=expected_diagnostics,
        )
        test_cases.append(grouped)

    return test_cases


def compute_stats(
    test_cases: list[TestCase],
    ty_version: Literal["new", "old"],
) -> Statistics:
    source = Source.NEW if ty_version == "new" else Source.OLD

    def increment(statistics: Statistics, grouped: TestCase) -> Statistics:
        eval = grouped.classify(source)
        statistics.true_positives += eval.true_positives
        statistics.false_positives += eval.false_positives
        statistics.false_negatives += eval.false_negatives
        statistics.total_diagnostics += len(grouped.diagnostics_by_source(source))
        return statistics

    return reduce(increment, test_cases, Statistics())


def render_test_cases(
    test_cases: list[TestCase],
    *,
    format: Literal["diff", "github"] = "diff",
) -> str:
    # Each entry is (title, test_case, source) where source=None means show both old
    # (removed) and new (added) — used for "changed" sections where the classification
    # is the same in both versions but the diagnostics themselves differ.
    entries: list[tuple[str, TestCase, Source | None]] = []

    for test_case in test_cases:
        change = test_case.change
        if change == Change.UNCHANGED:
            continue

        if test_case.optional:
            if change == Change.ADDED:
                entries.append((Change.ADDED.into_title(), test_case, Source.NEW))
            elif change == Change.REMOVED:
                entries.append((Change.REMOVED.into_title(), test_case, Source.OLD))
            elif change == Change.CHANGED:
                entries.append((Change.CHANGED.into_title(), test_case, None))
        else:
            if change == Change.ADDED:
                new_class = test_case.classify(Source.NEW).classification
                entries.append(
                    (new_class.into_title(verb="added"), test_case, Source.NEW)
                )
            elif change == Change.REMOVED:
                old_class = test_case.classify(Source.OLD).classification
                entries.append(
                    (old_class.into_title(verb="removed"), test_case, Source.OLD)
                )
            elif change == Change.CHANGED:
                old_class = test_case.classify(Source.OLD).classification
                new_class = test_case.classify(Source.NEW).classification
                if old_class == new_class:
                    # Same classification but different diagnostics: show a before/after diff
                    # in a single "changed" section rather than the confusing split into
                    # separate "removed" and "added" sections for the same classification.
                    entries.append(
                        (new_class.into_title(verb="changed"), test_case, None)
                    )
                else:
                    # Classification changed: show old under "[X] removed" and new under
                    # "[Y] added" since the status genuinely changed.
                    entries.append(
                        (old_class.into_title(verb="removed"), test_case, Source.OLD)
                    )
                    entries.append(
                        (new_class.into_title(verb="added"), test_case, Source.NEW)
                    )

    if not entries:
        return ""

    # Sort by priority then test-case key so groups are contiguous and stable.
    entries.sort(key=lambda e: (TITLE_PRIORITY.get(e[0], 99), e[0], e[1].key))

    _GITHUB_HEADER = ["| Location | Name | Message |", "|----------|------|---------|"]
    _GITHUB_CHANGED_HEADER = [
        "| Δ | Location | Name | Message |",
        "|---|----------|------|---------|",
    ]

    lines = []
    for title, group in groupby(entries, key=lambda e: e[0]):
        group_list = list(group)
        is_changed_section = group_list[0][2] is None

        lines.append(f"### {title}")
        lines.extend(["", "<details>", ""])

        if format == "diff":
            lines.append("```diff")
        elif is_changed_section:
            lines.extend(_GITHUB_CHANGED_HEADER)
        else:
            lines.extend(_GITHUB_HEADER)

        for _, tc, source in group_list:
            if source is None:
                # "Changed" entry: render old diagnostics (-) then new diagnostics (+).
                if format == "diff":
                    lines.append(tc._render_diff(tc.old, removed=True))
                    lines.append(tc._render_diff(tc.new, removed=False))
                else:
                    lines.append(tc._render_changed_row(tc.old, removed=True))
                    lines.append(tc._render_changed_row(tc.new, removed=False))
            else:
                diagnostics = tc.diagnostics_by_source(source)
                removed = source == Source.OLD
                if format == "diff":
                    lines.append(tc._render_diff(diagnostics, removed=removed))
                else:
                    lines.append(tc._render_row(diagnostics))

        if format == "diff":
            lines.append("```")
        lines.extend(["", "</details>", ""])

    return "\n".join(lines)


def file_is_fully_passing(test_cases: list[TestCase], source: Source) -> bool:
    """Return True if every test case for a file classifies as TP or TN for source."""
    return all(
        tc.classify(source).classification
        in (Classification.TRUE_POSITIVE, Classification.TRUE_NEGATIVE)
        for tc in test_cases
    )


def render_file_status_changes(test_cases: list[TestCase]) -> str:
    """Render a section listing files that newly achieve or lose fully-passing status."""

    def get_path(tc: TestCase) -> Path:
        for diags in (tc.new, tc.old, tc.expected):
            if diags:
                return diags[0].location.path
        raise ValueError(f"No diagnostics in test case {tc.key}")

    path_to_cases: dict[Path, list[TestCase]] = {}
    for tc in test_cases:
        path_to_cases.setdefault(get_path(tc), []).append(tc)

    newly_passing: list[Path] = []
    newly_failing: list[Path] = []

    for path, cases in sorted(path_to_cases.items()):
        old_passing = file_is_fully_passing(cases, Source.OLD)
        new_passing = file_is_fully_passing(cases, Source.NEW)
        if not old_passing and new_passing:
            newly_passing.append(path)
        elif old_passing and not new_passing:
            newly_failing.append(path)

    if not newly_passing and not newly_failing:
        return ""

    lines = []
    if newly_passing:
        lines.append("### Files now fully passing 🎉")
        lines.append("")
        for path in newly_passing:
            url = CONFORMANCE_DIR_WITH_README + f"tests/{path.name}"
            lines.append(f"- [{path.name}]({url})")
        lines.append("")
    if newly_failing:
        lines.append("### Files now failing ❌")
        lines.append("")
        for path in newly_failing:
            url = CONFORMANCE_DIR_WITH_README + f"tests/{path.name}"
            lines.append(f"- [{path.name}]({url})")
        lines.append("")

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


def render_summary(test_cases: list[TestCase], *, force_summary_table: bool) -> str:
    def format_metric(diff: float, old: float, new: float):
        if diff > 0:
            return f"increased from {old:.2%} to {new:.2%}"
        if diff < 0:
            return f"decreased from {old:.2%} to {new:.2%}"
        return f"held steady at {old:.2%}"

    old = compute_stats(test_cases, ty_version="old")
    new = compute_stats(test_cases, ty_version="new")

    assert new.true_positives > 0, (
        "Expected ty to have at least one true positive.\n"
        f"Sample of grouped diagnostics: {test_cases[:5]}"
    )

    precision_change = new.precision - old.precision
    recall_change = new.recall - old.recall
    true_pos_change = new.true_positives - old.true_positives
    false_pos_change = new.false_positives - old.false_positives
    false_neg_change = new.false_negatives - old.false_negatives
    total_change = new.total_diagnostics - old.total_diagnostics

    base_header = f"[Typing conformance results]({CONFORMANCE_DIR_WITH_README})"

    if not force_summary_table and all(
        diag.change is Change.UNCHANGED for diag in test_cases
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

    extra_search_paths = [tests_dir / "tests"]

    old = collect_ty_diagnostics(
        ty_path=args.old_ty,
        test_files=test_files,
        source=Source.OLD,
        python_version=args.python_version,
        extra_search_paths=extra_search_paths,
    )

    new = collect_ty_diagnostics(
        ty_path=args.new_ty,
        test_files=test_files,
        source=Source.NEW,
        python_version=args.python_version,
        extra_search_paths=extra_search_paths,
    )

    grouped = group_diagnostics_by_key(
        old=old,
        new=new,
        expected=expected,
    )

    rendered = "\n\n".join(
        filter(
            None,
            [
                render_summary(grouped, force_summary_table=args.force_summary_table),
                render_file_status_changes(grouped),
                render_test_cases(grouped, format=args.format),
            ],
        )
    )

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
        print(rendered, file=sys.stderr)
    else:
        print(rendered)


if __name__ == "__main__":
    main()

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
import html
import json
import os
import re
import subprocess
import sys
import tomllib
from collections import defaultdict
from collections.abc import Sequence, Set as AbstractSet
from dataclasses import dataclass
from enum import StrEnum, auto
from itertools import chain, groupby
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

GITHUB_HEADER = [
    "<table>",
    "<tr><th>Test case</th><th>Δ</th><th>Location</th><th>Name</th><th>Message</th></tr>",
]
GITHUB_FOOTER = ["</table>"]
SUMMARY_NOTE = """
    Each test case represents one expected error annotation or a group of annotations
    sharing a tag. Counts are per test case, not per diagnostic — multiple diagnostics
    on the same line count as one. Required annotations (`E`) are true positives when
    ty flags the expected location and false negatives when it does not. Optional
    annotations (`E?`) are true positives when flagged but true negatives (not false
    negatives) when not. Tagged annotations (`E[tag]`) require ty to flag exactly one
    of the tagged lines; tagged multi-annotations (`E[tag+]`) allow any number up to
    the tag count. Flagging unexpected locations counts as a false positive.
"""
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
        link = CONFORMANCE_URL.format(filename=file, line=self.positions.begin.line)
        return f"[{file}:{self.positions.begin.line}:{self.positions.begin.column}]({link})"

    def as_html_link(self) -> str:
        file = self.path.name
        link = CONFORMANCE_URL.format(filename=file, line=self.positions.begin.line)
        return f'<a href="{link}">{self.positions.begin.line}:{self.positions.begin.column}</a>'


@dataclass(kw_only=True, slots=True)
class TyDiagnostic:
    """A diagnostic emitted by a ty version (old or new) during a conformance run."""

    check_name: str
    description: str
    severity: str
    location: Location
    source: Source

    def __post_init__(self) -> None:
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
        )

    @property
    def severity_for_display(self) -> str:
        return {
            "major": "error",
            "minor": "warning",
        }.get(self.severity, "unknown")


@dataclass(kw_only=True, slots=True)
class ExpectedError:
    """An error annotation parsed from a conformance test file (e.g. ``# E: ...``)."""

    description: str
    location: Location
    optional: bool
    # tag identifying an error that can occur on multiple lines
    tag: str | None
    # True if one or more errors can occur on lines with the same tag
    multi: bool

    @property
    def key(self) -> str:
        """Key to group expected errors by path and beginning line or path and tag."""
        return (
            f"{self.location.path.as_posix()}:{self.location.positions.begin.line}"
            if self.tag is None
            else f"{self.location.path.as_posix()}:{self.tag}"
        )


def diagnostics_are_equivalent(a: list[TyDiagnostic], b: list[TyDiagnostic]) -> bool:
    """Compare two diagnostic lists for equality, ignoring the ``source`` field."""

    def fingerprint(d: TyDiagnostic) -> tuple:
        return (
            d.check_name,
            d.description,
            d.severity,
            str(d.location.path),
            d.location.positions.begin.line,
            d.location.positions.begin.column,
        )

    return sorted(map(fingerprint, a)) == sorted(map(fingerprint, b))


@dataclass(kw_only=True, slots=True)
class TestCase:
    key: str
    old: list[TyDiagnostic]
    new: list[TyDiagnostic]
    expected: list[ExpectedError]

    @property
    def change(self) -> Change:
        if self.new and not self.old:
            return Change.ADDED
        elif self.old and not self.new:
            return Change.REMOVED
        elif (
            self.old and self.new and not diagnostics_are_equivalent(self.old, self.new)
        ):
            return Change.CHANGED
        else:
            return Change.UNCHANGED

    @property
    def optional(self) -> bool:
        return bool(self.expected) and all(e.optional for e in self.expected)

    @property
    def multi(self) -> bool:
        return bool(self.expected) and all(e.multi for e in self.expected)

    @property
    def path(self) -> Path:
        """Return the source file path for this test case."""
        for diags in (self.new, self.old, self.expected):
            if diags:
                return diags[0].location.path
        raise ValueError(f"No diagnostics in test case {self.key}")

    def diagnostics_by_source(self, source: Source) -> list[TyDiagnostic]:
        return self.old if source == Source.OLD else self.new

    def classify(self, source: Source) -> Classification:
        diagnostics = self.diagnostics_by_source(source)

        if diagnostics:
            if self.optional:
                return Classification.TRUE_POSITIVE
            if self.expected:
                distinct_lines = len(
                    {d.location.positions.begin.line for d in diagnostics}
                )
                expected_max = len(self.expected) if self.multi else 1
                if 1 <= distinct_lines <= expected_max:
                    return Classification.TRUE_POSITIVE
                else:
                    return Classification.FALSE_POSITIVE
            else:
                return Classification.FALSE_POSITIVE

        elif self.expected:
            if self.optional:
                return Classification.TRUE_NEGATIVE
            return Classification.FALSE_NEGATIVE

        else:
            return Classification.TRUE_NEGATIVE


def tc_location_cell(tc: TestCase, *, rowspan: int, source: Source | None) -> str:
    """Build a <td> for the test-case-level location, spanning ``rowspan`` rows."""
    all_diags = tc.old + tc.new if source is None else tc.diagnostics_by_source(source)

    if not all_diags:
        return f'<td rowspan="{rowspan}"></td>' if rowspan > 1 else "<td></td>"

    min_line = min(d.location.positions.begin.line for d in all_diags)
    max_line = max(d.location.positions.begin.line for d in all_diags)
    filename = all_diags[0].location.path.name

    if min_line == max_line:
        url = CONFORMANCE_URL.format(filename=filename, line=min_line)
        display = f"{filename}:{min_line}"
    else:
        url = f"{CONFORMANCE_DIR_WITH_README}tests/{filename}#L{min_line}-L{max_line}"
        display = f"{filename}:{min_line}:{max_line}"

    rowspan_attr = f' rowspan="{rowspan}"' if rowspan > 1 else ""
    return f'<td{rowspan_attr} align="center" valign="middle"><a href="{url}">{display}</a></td>'


def render_html_test_case_rows(tc: TestCase, *, source: Source | None) -> list[str]:
    """Render one HTML <tr> per diagnostic for a test case.

    The test-case location cell spans all rows. The delta cell (-/+) spans
    the diagnostics for its side. For changed entries, old rows come first,
    then new rows, each group with its own spanning delta cell.
    """

    def delta_cell(symbol: str, rowspan: int) -> str:
        rowspan_attr = f' rowspan="{rowspan}"' if rowspan > 1 else ""
        return f'<td{rowspan_attr} align="center" valign="middle">{symbol}</td>'

    def diag_row(d: TyDiagnostic, prepend: str = "") -> str:
        name = html.escape(d.check_name)
        desc = html.escape(d.description.replace("\\|", "|"))
        return (
            f"<tr>{prepend}"
            f"<td>{d.location.as_html_link()}</td>"
            f"<td>{name}</td>"
            f"<td>{desc}</td>"
            f"</tr>"
        )

    def render_group(
        diags: list[TyDiagnostic], symbol: str, loc_prefix: str = ""
    ) -> None:
        for i, d in enumerate(diags):
            prepend = (loc_prefix + delta_cell(symbol, len(diags))) if i == 0 else ""
            rows.append(diag_row(d, prepend))

    rows: list[str] = []

    if source is None:
        old_diags, new_diags = tc.old, tc.new
        loc = tc_location_cell(tc, rowspan=len(old_diags) + len(new_diags), source=None)
        render_group(old_diags, "-", loc)
        render_group(new_diags, "+")
    else:
        diags = tc.diagnostics_by_source(source)
        loc = tc_location_cell(tc, rowspan=len(diags), source=source)
        render_group(diags, "-" if source == Source.OLD else "+", loc)

    return rows


def render_diff_row(diagnostics: list[TyDiagnostic], *, removed: bool = False) -> str:
    sign = "-" if removed else "+"
    return "\n".join(f"{sign} {d}" for d in diagnostics)


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


@dataclass(kw_only=True, slots=True)
class DiagnosticEntry:
    """A test case bound to the section title and source side it should be rendered under."""

    title: str
    test_case: TestCase
    # None means show both old (removed) and new (added) in a single "changed" section.
    source: Source | None


@dataclass(kw_only=True, slots=True)
class FileStats:
    path: Path
    old: Statistics
    new: Statistics

    @property
    def old_passes(self) -> bool:
        return self.old.false_positives == 0 and self.old.false_negatives == 0

    @property
    def new_passes(self) -> bool:
        return self.new.false_positives == 0 and self.new.false_negatives == 0

    @property
    def total_change(self) -> int:
        return (
            abs(self.new.true_positives - self.old.true_positives)
            + abs(self.new.false_positives - self.old.false_positives)
            + abs(self.new.false_negatives - self.old.false_negatives)
        )


def collect_expected_diagnostics(test_files: Sequence[Path]) -> list[ExpectedError]:
    errors: list[ExpectedError] = []
    for file in test_files:
        for idx, line in enumerate(file.read_text().splitlines(), 1):
            if match := re.search(CONFORMANCE_ERROR_PATTERN, line):
                errors.append(
                    ExpectedError(
                        description=(match.group("description") or "Missing"),
                        location=Location(
                            path=file,
                            positions=Positions(
                                begin=Position(
                                    line=idx,
                                    column=match.start(),
                                ),
                                end=Position(
                                    line=idx,
                                    column=match.end(),
                                ),
                            ),
                        ),
                        optional=match.group("optional") is not None,
                        tag=(
                            f"{file.name}:{match.group('tag')}"
                            if match.group("tag")
                            else None
                        ),
                        multi=match.group("multi") is not None,
                    )
                )

    assert errors, "Failed to discover any expected diagnostics!"
    return errors


def collect_ty_diagnostics(
    ty_path: list[str],
    source: Source,
    test_files: Sequence[Path],
    python_version: str = "3.12",
    extra_search_paths: Sequence[Path] = (),
) -> list[TyDiagnostic]:
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

    return [
        TyDiagnostic.from_gitlab_output(dct, source=source)
        for dct in json.loads(process.stdout)
        if dct["severity"] == "major"
    ]


def group_diagnostics_by_key(
    old: list[TyDiagnostic],
    new: list[TyDiagnostic],
    expected: list[ExpectedError],
) -> list[TestCase]:
    # Build a lookup from (filename, line) to tag so ty diagnostics on a tagged
    # line can be grouped with all other expected errors sharing that tag.
    tagged_lines: dict[tuple[str, int], str] = {
        (e.location.path.name, e.location.positions.begin.line): e.tag
        for e in expected
        if e.tag is not None
    }

    def ty_key(diag: TyDiagnostic) -> str:
        tag = tagged_lines.get(
            (diag.location.path.name, diag.location.positions.begin.line)
        )
        return (
            f"{diag.location.path.as_posix()}:{tag}"
            if tag is not None
            else f"{diag.location.path.as_posix()}:{diag.location.positions.begin.line}"
        )

    old_by_key: defaultdict[str, list[TyDiagnostic]] = defaultdict(list)
    new_by_key: defaultdict[str, list[TyDiagnostic]] = defaultdict(list)
    expected_by_key: defaultdict[str, list[ExpectedError]] = defaultdict(list)

    for diag in old:
        old_by_key[ty_key(diag)].append(diag)
    for diag in new:
        new_by_key[ty_key(diag)].append(diag)
    for err in expected:
        expected_by_key[err.key].append(err)

    all_keys = sorted(old_by_key.keys() | new_by_key.keys() | expected_by_key.keys())
    return [
        TestCase(
            key=key,
            old=old_by_key[key],
            new=new_by_key[key],
            expected=expected_by_key[key],
        )
        for key in all_keys
    ]


def compute_stats(test_cases: list[TestCase], source: Source) -> Statistics:
    stats = Statistics()
    for tc in test_cases:
        match tc.classify(source):
            case Classification.TRUE_POSITIVE:
                stats.true_positives += 1
            case Classification.FALSE_POSITIVE:
                stats.false_positives += 1
            case Classification.FALSE_NEGATIVE:
                stats.false_negatives += 1
            case Classification.TRUE_NEGATIVE:
                pass
        stats.total_diagnostics += len(tc.diagnostics_by_source(source))
    return stats


def collect_diagnostic_entries(test_cases: list[TestCase]) -> list[DiagnosticEntry]:
    """Classify each changed test case and assign it to a titled section."""
    entries: list[DiagnosticEntry] = []

    for tc in test_cases:
        change = tc.change
        if change == Change.UNCHANGED:
            continue

        if tc.optional:
            if change == Change.ADDED:
                entries.append(
                    DiagnosticEntry(
                        title=Change.ADDED.into_title(), test_case=tc, source=Source.NEW
                    )
                )
            elif change == Change.REMOVED:
                entries.append(
                    DiagnosticEntry(
                        title=Change.REMOVED.into_title(),
                        test_case=tc,
                        source=Source.OLD,
                    )
                )
            elif change == Change.CHANGED:
                entries.append(
                    DiagnosticEntry(
                        title=Change.CHANGED.into_title(), test_case=tc, source=None
                    )
                )
        else:
            if change == Change.ADDED:
                new_class = tc.classify(Source.NEW)
                entries.append(
                    DiagnosticEntry(
                        title=new_class.into_title(verb="added"),
                        test_case=tc,
                        source=Source.NEW,
                    )
                )
            elif change == Change.REMOVED:
                old_class = tc.classify(Source.OLD)
                entries.append(
                    DiagnosticEntry(
                        title=old_class.into_title(verb="removed"),
                        test_case=tc,
                        source=Source.OLD,
                    )
                )
            elif change == Change.CHANGED:
                old_class = tc.classify(Source.OLD)
                new_class = tc.classify(Source.NEW)
                if old_class == new_class:
                    # Same classification but different diagnostics: one "changed" section.
                    entries.append(
                        DiagnosticEntry(
                            title=new_class.into_title(verb="changed"),
                            test_case=tc,
                            source=None,
                        )
                    )
                else:
                    # Classification changed: split into separate removed/added sections.
                    entries.append(
                        DiagnosticEntry(
                            title=old_class.into_title(verb="removed"),
                            test_case=tc,
                            source=Source.OLD,
                        )
                    )
                    entries.append(
                        DiagnosticEntry(
                            title=new_class.into_title(verb="added"),
                            test_case=tc,
                            source=Source.NEW,
                        )
                    )

    entries.sort(
        key=lambda e: (TITLE_PRIORITY.get(e.title, 99), e.title, e.test_case.key)
    )
    return entries


def render_test_cases(
    test_cases: list[TestCase],
    *,
    format: Literal["diff", "github"] = "diff",
) -> str:
    entries = collect_diagnostic_entries(test_cases)
    if not entries:
        return ""

    lines = []
    for title, group in groupby(entries, key=lambda e: e.title):
        group_list = list(group)
        n = len(group_list)

        lines.append(f"### {title} ({n})")
        lines.extend(
            [
                "",
                "<details>",
                f"<summary>{n} {'diagnostic' if n == 1 else 'diagnostics'}</summary>",
                "",
            ]
        )

        if format == "diff":
            lines.append("```diff")
        else:
            lines.extend(GITHUB_HEADER)

        for entry in group_list:
            tc, source = entry.test_case, entry.source
            if source is None:
                if format == "diff":
                    lines.append(render_diff_row(tc.old, removed=True))
                    lines.append(render_diff_row(tc.new, removed=False))
                else:
                    lines.extend(render_html_test_case_rows(tc, source=None))
            else:
                if format == "diff":
                    lines.append(
                        render_diff_row(
                            tc.diagnostics_by_source(source),
                            removed=source == Source.OLD,
                        )
                    )
                else:
                    lines.extend(render_html_test_case_rows(tc, source=source))

        if format == "diff":
            lines.append("```")
        else:
            lines.extend(GITHUB_FOOTER)
        lines.extend(["", "</details>", ""])

    return "\n".join(lines)


def render_file_stats_table(test_cases: list[TestCase]) -> str:
    """Render a per-file breakdown showing only files whose TP/FP/FN counts changed."""
    path_to_cases: dict[Path, list[TestCase]] = {}
    for tc in test_cases:
        path_to_cases.setdefault(tc.path, []).append(tc)

    def fmt(old: int, new: int, *, greater_is_better: bool = True) -> str:
        if old == new:
            return str(new)
        diff = new - old
        improved = diff > 0 if greater_is_better else diff < 0
        indicator = " ✅" if improved else " ❌"
        return f"{new} ({diff:+}){indicator}"

    # Collect per-file data; track totals across all files regardless of change.
    file_stats: list[FileStats] = []
    old_totals = Statistics()
    new_totals = Statistics()
    passing = 0
    total_files = 0

    for path, cases in path_to_cases.items():
        fs = FileStats(
            path=path,
            old=compute_stats(cases, Source.OLD),
            new=compute_stats(cases, Source.NEW),
        )
        old_totals.true_positives += fs.old.true_positives
        old_totals.false_positives += fs.old.false_positives
        old_totals.false_negatives += fs.old.false_negatives
        new_totals.true_positives += fs.new.true_positives
        new_totals.false_positives += fs.new.false_positives
        new_totals.false_negatives += fs.new.false_negatives
        passing += fs.new_passes
        total_files += 1
        file_stats.append(fs)

    changed = [fs for fs in file_stats if fs.total_change > 0]
    if not changed:
        return ""

    changed.sort(key=lambda fs: (-fs.total_change, fs.path.name))

    rows = []
    for fs in changed:
        if fs.new_passes and not fs.old_passes:
            status = "✅ Newly Passing 🎉"
        elif fs.old_passes and not fs.new_passes:
            status = "❌ Regressed"
        elif fs.new_passes:
            status = "✅"
        else:
            status = "❌"
        url = CONFORMANCE_DIR_WITH_README + f"tests/{fs.path.name}"
        rows.append(
            f"| [{fs.path.name}]({url})"
            f" | {fmt(fs.old.true_positives, fs.new.true_positives, greater_is_better=True)}"
            f" | {fmt(fs.old.false_positives, fs.new.false_positives, greater_is_better=False)}"
            f" | {fmt(fs.old.false_negatives, fs.new.false_negatives, greater_is_better=False)}"
            f" | {status} |"
        )

    totals_row = (
        f"| **Total**"
        f" | **{fmt(old_totals.true_positives, new_totals.true_positives, greater_is_better=True)}**"
        f" | **{fmt(old_totals.false_positives, new_totals.false_positives, greater_is_better=False)}**"
        f" | **{fmt(old_totals.false_negatives, new_totals.false_negatives, greater_is_better=False)}**"
        f" | {passing}/{total_files} |"
    )

    lines = [
        "### Files changed",
        "",
        '<div align="center">',
        "",
        "| File | True Positives | False Positives | False Negatives | Status |",
        "|------|----|----|----|--------|",
        *rows,
        totals_row,
        "",
        "</div>",
        "",
    ]
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
    old = compute_stats(test_cases, Source.OLD)
    new = compute_stats(test_cases, Source.NEW)

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

    summary_note = " ".join(SUMMARY_NOTE.split())

    return dedent(
        f"""
        ## {header}


        ### Summary

        {summary_note}

        <div align="center">

        | Metric | Old | New | Diff | Outcome |
        |--------|-----|-----|------|---------|
        | True Positives  | {old.true_positives} | {new.true_positives} | {true_pos_change:+} | {true_pos_diff} |
        | False Positives | {old.false_positives} | {new.false_positives} | {false_pos_change:+} | {false_pos_diff} |
        | False Negatives | {old.false_negatives} | {new.false_negatives} | {false_neg_change:+} | {false_neg_diff} |
        | Total Diagnostics | {old.total_diagnostics} | {new.total_diagnostics} | {total_change:+} | {total_diff} |
        | Precision | {old.precision:.2%} | {new.precision:.2%} | {precision_change:+.2%} | {precision_diff} |
        | Recall | {old.recall:.2%} | {new.recall:.2%} | {recall_change:+.2%} | {recall_diff} |

        </div>

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
                render_file_stats_table(grouped),
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

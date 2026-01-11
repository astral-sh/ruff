from __future__ import annotations

import argparse
import itertools as it
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from enum import Flag, StrEnum, auto
from functools import reduce
from operator import attrgetter, or_
from pathlib import Path
from textwrap import dedent
from typing import Any, Self

CONFORMANCE_ERROR_PATTERN = re.compile(r"#\s*E(?:\s*(?::\s*(.+)|\[(.+)\]))?\s*")


class Source(Flag):
    OLD = auto()
    NEW = auto()
    EXPECTED = auto()


class Classification(StrEnum):
    TRUE_POSITIVE = auto()
    FALSE_POSITIVE = auto()
    TRUE_NEGATIVE = auto()
    FALSE_NEGATIVE = auto()


@dataclass
class Position:
    line: int
    column: int


@dataclass
class Positions:
    begin: Position
    end: Position


@dataclass
class Location:
    path: str
    positions: Positions


@dataclass
class Diagnostic:
    check_name: str
    description: str
    severity: str
    fingerprint: str | None
    location: Location
    source: Source

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
    def key(self):
        """Key to group diagnostics by path and beginning line."""
        return f"{self.location.path}:{self.location.positions.begin.line}"

    @property
    def severity_for_display(self) -> str:
        return {
            "major": "error",
            "minor": "warning",
        }.get(self.severity, "unknown")

    def to_concise(self) -> str:
        return (
            f"{self.location.path}:{self.location.positions.begin.line}:"
            f"{self.location.positions.begin.column}: "
            f"{self.severity_for_display}[{self.check_name}] {self.description}"
        )


@dataclass
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

    def display(self) -> str:
        match self.classification:
            case Classification.TRUE_POSITIVE | Classification.FALSE_POSITIVE:
                assert self.new is not None
                return f"+ {self.new.to_concise()}"

            case Classification.FALSE_NEGATIVE | Classification.TRUE_NEGATIVE:
                if self.old is not None:
                    return f"- {self.old.to_concise()}"
                elif self.expected is not None:
                    return f"- {self.expected.to_concise()}"
                else:
                    return ""
            case _:
                raise ValueError(f"Unexpected classification: {self.classification}")


@dataclass
class Statistics:
    tp: int = 0
    fp: int = 0
    fn: int = 0

    @property
    def precision(self) -> float:
        if self.tp + self.fp > 0:
            return self.tp / (self.tp + self.fp)
        return 0.0

    @property
    def recall(self) -> float:
        if self.tp + self.fn > 0:
            return self.tp / (self.tp + self.fn)
        else:
            return 0.0

    @property
    def total(self) -> int:
        return self.tp + self.fp


def collect_expected_diagnostics(path: Path) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    for file in path.resolve().rglob("*.py"):
        for idx, line in enumerate(file.read_text().splitlines(), 1):
            if error := re.search(CONFORMANCE_ERROR_PATTERN, line):
                diagnostics.append(
                    Diagnostic(
                        check_name="conformance",
                        description=error.group(1) or "Missing",
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
    target_path: str = ".",
    python_version: str = "3.12",
) -> list[Diagnostic]:
    process = subprocess.run(
        [
            *ty_path,
            "check",
            f"--python-version={python_version}",
            "--output-format=gitlab",
            "--exit-zero",
            target_path,
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
    for key, group in it.groupby(sorted_diagnostics, key=attrgetter("key")):
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
        num_errors = len(
            [g for g in grouped_diagnostics if source.EXPECTED in g.sources]  # ty:ignore[unsupported-operator]
        )
        return Statistics(tp=num_errors, fp=0, fn=0)

    def increment(statistics: Statistics, grouped: GroupedDiagnostics) -> Statistics:
        if (source in grouped.sources) and (Source.EXPECTED in grouped.sources):
            statistics.tp += 1
        elif source in grouped.sources:
            statistics.fp += 1
        else:
            statistics.fn += 1
        return statistics

    return reduce(increment, grouped_diagnostics, Statistics())


def render_grouped_diagnostics(
    grouped: list[GroupedDiagnostics], changed_only: bool = True
) -> str:
    if changed_only:
        grouped = [diag for diag in grouped if diag.changed]
    sorted_by_class = sorted(
        grouped,
        key=attrgetter("classification"),
        reverse=True,
    )

    lines = []
    for classification, group in it.groupby(
        sorted_by_class, key=attrgetter("classification")
    ):
        group = list(group)

        lines.append(f"## {classification.value.replace('_', ' ').title()}s:")
        lines.append("")
        lines.append("```diff")

        for diag in group:
            lines.append(diag.display())

        lines.append("```")

    return "\n".join(lines)


def render_summary(grouped_diagnostics: list[GroupedDiagnostics]):
    def pct(value):
        return f"{value:.2%}"

    def trend(value):
        if value == 0:
            return "does not change"
        return "improves" if value > 0 else "regresses"

    old = compute_stats(grouped_diagnostics, source=Source.OLD)
    new = compute_stats(grouped_diagnostics, source=Source.NEW)

    precision_delta = new.precision - old.precision
    recall_delta = new.recall - old.recall

    table = dedent(
        f"""
        ## Typing Conformance

        ### Summary

        | Metric     | Old | New | Δ |
        |------------|-----|-----|---|
        | True Positives | {old.tp} | {new.tp} | {old.tp - new.tp} |
        | False Positives | {old.fp} | {new.fp} | {new.fp - old.fp} |
        | False Negatives | {old.fn} | {new.fn} | {new.fn - old.fn} |
        | Precision  | {old.precision:.2} | {new.precision:.2} | {pct(precision_delta)} |
        | Recall     | {old.recall:.2} | {new.recall:.2} | {pct(recall_delta)} |
        | Total      | {old.total} | {new.total} | {new.total - old.total} |

        """
    )

    summary = (
        f"Compared to the current merge base, this PR {trend(precision_delta)} precision "
        f"and {trend(recall_delta)} recall (TP: {new.tp - old.tp}, FP: {new.fp - old.fp}, FN: {new.fn - old.fn}))."
    )

    return "\n".join([table, summary])


def parse_args():
    parser = argparse.ArgumentParser(
        description="Run typing conformance tests and compare results between two ty versions",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=dedent("""
            Examples:
              # Compare two specific ty versions
              %(prog)s --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7

              # Use local ty builds
              %(prog)s --old-ty ./target/debug/ty-old --new-ty ./target/debug/ty-new

              # Custom test directory
              %(prog)s --target-path custom/tests --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7

              # Show all diagnostics (not just changed ones)
              %(prog)s --all --old-ty uvx ty@0.0.1a35 --new-ty uvx ty@0.0.7
        """),
    )

    parser.add_argument(
        "--old-ty",
        nargs="+",
        default=["uvx", "ty@0.0.1a35"],
        help="Command to run old version of ty (default: uvx ty@0.0.1a35)",
    )

    parser.add_argument(
        "--new-ty",
        nargs="+",
        default=["uvx", "ty@0.0.7"],
        help="Command to run new version of ty (default: uvx ty@0.0.7)",
    )

    parser.add_argument(
        "--target-path",
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
        "--output",
        type=Path,
        help="Write output to file instead of stdout",
    )

    return parser.parse_args()


def main():
    args = parse_args()

    expected = collect_expected_diagnostics(args.target_path)

    old = collect_ty_diagnostics(
        ty_path=args.old_ty,
        target_path=str(args.target_path),
        source=Source.OLD,
        python_version=args.python_version,
    )

    new = collect_ty_diagnostics(
        ty_path=args.new_ty,
        target_path=str(args.target_path),
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
            render_grouped_diagnostics(grouped, changed_only=not args.all),
        ]
    )

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
    else:
        print(rendered)


if __name__ == "__main__":
    main()

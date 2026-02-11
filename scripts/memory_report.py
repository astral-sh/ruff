"""
Compare memory usage reports between two ty versions and generate a PR comment.

This script can be used in two modes:

1. Report comparison mode: Reads pre-generated JSON memory reports and compares them.
2. Full run mode: Clones projects, builds ty, runs memory tests, and generates comparison.

Examples:
    # Compare pre-generated memory reports
    %(prog)s compare --old-dir old_reports/ --new-dir new_reports/

    # Full run: clone projects, build ty, run memory tests
    %(prog)s run --old-ty ./ty-old --new-ty ./ty-new

    # Write output to a file
    %(prog)s compare --old-dir old_reports/ --new-dir new_reports/ --output memory_diff.md
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Final, Self

# Known projects with their Git URLs for memory testing.
KNOWN_PROJECTS: Final[Mapping[str, str]] = {
    "flake8": "https://github.com/PyCQA/flake8",
    "sphinx": "https://github.com/sphinx-doc/sphinx",
    "prefect": "https://github.com/PrefectHQ/prefect",
    "trio": "https://github.com/python-trio/trio",
}


@dataclass(slots=True, kw_only=True)
class MemoryReport:
    """Memory report from a single ty run."""

    total_bytes: int
    struct_metadata_bytes: int
    struct_fields_bytes: int
    memo_metadata_bytes: int
    memo_fields_bytes: int
    structs: list[dict[str, Any]] = field(default_factory=list)
    queries: list[dict[str, Any]] = field(default_factory=list)

    @classmethod
    def from_json(cls, path: Path) -> Self:
        """Load a memory report from a JSON file."""
        with open(path) as f:
            data = json.load(f)
        return cls(
            total_bytes=data["total_bytes"],
            struct_metadata_bytes=data["struct_metadata_bytes"],
            struct_fields_bytes=data["struct_fields_bytes"],
            memo_metadata_bytes=data["memo_metadata_bytes"],
            memo_fields_bytes=data["memo_fields_bytes"],
            structs=data.get("structs", []),
            queries=data.get("queries", []),
        )


@dataclass(slots=True, kw_only=True)
class ProjectComparison:
    """Comparison of memory usage between old and new ty versions for a project."""

    name: str
    old: MemoryReport
    new: MemoryReport

    @property
    def total_diff_bytes(self) -> int:
        return self.new.total_bytes - self.old.total_bytes


def format_bytes(bytes: int) -> str:
    """Format bytes as a human-readable size."""
    bytes: float = float(bytes)

    for unit in ("B", "kB", "MB"):
        if abs(bytes) < 1024.0:
            return f"{bytes:.2f}{unit}"
        bytes /= 1024.0

    return f"{bytes:.2f}GB"


def format_diff(*, old_bytes: int, new_bytes: int) -> str:
    """Format a difference with percentage and direction indicator."""
    diff = new_bytes - old_bytes

    if diff == 0:
        return "-"

    sign = "+" if diff > 0 else ""

    if old_bytes == 0:
        return f"{sign}{format_bytes(diff)} (new)"

    return f"{sign}{diff / old_bytes:.2%} ({format_bytes(abs(diff))})"


def format_outcome(*, old_bytes: int, new_bytes: int) -> str:
    """Format the outcome indicator."""
    diff = new_bytes - old_bytes
    if diff > 0:
        return "⏫"
    elif diff < 0:
        return "⬇️"
    else:
        return "✅"


def load_reports_from_directory(directory: Path) -> dict[str, MemoryReport]:
    """Load all JSON memory reports from a directory."""
    reports = {}
    for path in directory.glob("*.json"):
        project_name = path.stem
        reports[project_name] = MemoryReport.from_json(path)
    return reports


def item_total_bytes(item: dict[str, Any]) -> int:
    """Get total bytes (metadata + fields) for a struct or query item."""
    return item.get("metadata_bytes", 0) + item.get("fields_bytes", 0)


def diff_items(
    *, old_items: list[dict[str, Any]], new_items: list[dict[str, Any]]
) -> list[tuple[str, int, int]]:
    """Diff two lists of struct/query items by name.

    Returns a list of (name, old_bytes, new_bytes) sorted by absolute diff descending.
    """
    old_by_name = {item["name"]: item for item in old_items}
    new_by_name = {item["name"]: item for item in new_items}

    all_names = old_by_name.keys() | new_by_name.keys()

    diffs = []
    for name in all_names:
        old_bytes = item_total_bytes(old_by_name[name]) if name in old_by_name else 0
        new_bytes = item_total_bytes(new_by_name[name]) if name in new_by_name else 0
        if old_bytes != new_bytes:
            diffs.append((name.replace(" ", ""), old_bytes, new_bytes))

    diffs.sort(key=lambda x: abs(x[2] - x[1]), reverse=True)
    return diffs


# Maximum number of changed items to show per category in the detailed breakdown
MAX_CHANGED_ITEMS: Final = 15


def render_summary(projects: list[ProjectComparison]) -> str:
    """Render a summary of all project comparisons."""
    if not projects:
        return "No memory reports to compare."

    projects.sort(key=lambda p: p.total_diff_bytes, reverse=True)

    # Suppress the memory report if no project had any top-line changes >10KB
    any_increased = any(p.total_diff_bytes > 10_000 for p in projects)
    any_decreased = any(p.total_diff_bytes < -10_000 for p in projects)
    any_changed = any_increased or any_decreased

    lines = ["## Memory usage report", ""]

    if any_changed:
        lines.extend(
            [
                "### Summary",
                "",
                "| Project | Old | New | Diff | Outcome |",
                "|---------|-----|-----|------|---------|",
            ]
        )

        for proj in projects:
            outcome = format_outcome(
                old_bytes=proj.old.total_bytes, new_bytes=proj.new.total_bytes
            )

            lines.append(
                f"| {proj.name} | {format_bytes(proj.old.total_bytes)} | "
                f"{format_bytes(proj.new.total_bytes)} | "
                f"{format_diff(old_bytes=proj.old.total_bytes, new_bytes=proj.new.total_bytes)} | {outcome} |"
            )

        lines.extend(
            [
                "",
                "### Significant changes",
                "",
                "<details>",
                "<summary>Click to expand detailed breakdown</summary>",
                "",
            ]
        )

        for proj in projects:
            item_diffs = diff_items(
                old_items=proj.old.structs + proj.old.queries,
                new_items=proj.new.structs + proj.new.queries,
            )

            if not item_diffs:
                continue

            lines.extend(
                [
                    f"### {proj.name}",
                    "",
                    "| Name | Old | New | Diff | Outcome |",
                    "|------|-----|-----|------|---------|",
                ]
            )

            for name, old_bytes, new_bytes in item_diffs[:MAX_CHANGED_ITEMS]:
                outcome = format_outcome(
                    old_bytes=proj.old.total_bytes, new_bytes=proj.new.total_bytes
                )

                lines.append(
                    f"| `{name}` | {format_bytes(old_bytes)} | "
                    f"{format_bytes(new_bytes)} | "
                    f"{format_diff(old_bytes=old_bytes, new_bytes=new_bytes)} |"
                    f"{outcome} |"
                )

            remaining = len(item_diffs) - MAX_CHANGED_ITEMS
            if remaining > 0:
                lines.append(f"| ... | | | *{remaining} more* |")

            lines.append("")

        lines.extend(
            [
                "</details>",
                "",
            ]
        )
    else:
        lines.append("Memory usage unchanged ✅")

    return "\n".join(lines)


def clone_project(*, name: str, url: str, dest: Path) -> Path:
    """Clone a project from Git. Returns the path to the cloned project."""
    project_path = dest / name
    if project_path.exists():
        print(f"Project {name} already exists at {project_path}", file=sys.stderr)
        return project_path

    print(f"Cloning {name} from {url}...", file=sys.stderr)
    subprocess.run(
        ["git", "clone", "--depth=1", url, str(project_path)],
        check=True,
        capture_output=True,
    )
    return project_path


def run_ty_memory_check(
    *,
    ty_path: str,
    project_path: Path,
    output_path: Path,
) -> None:
    """Run ty on a project and capture memory report to a file."""
    env = os.environ.copy()
    env["TY_MEMORY_REPORT"] = "json"
    env["TY_MAX_PARALLELISM"] = "1"  # For deterministic memory numbers

    print(f"Running {ty_path} on {project_path.name}...", file=sys.stderr)
    result = subprocess.run(
        [ty_path, "check", str(project_path), "--exit-zero"],
        capture_output=True,
        text=True,
        env=env,
    )
    # Write stdout (JSON memory report) to file
    output_path.write_text(result.stdout, encoding="utf-8")


def run_memory_tests(
    *,
    old_ty: str,
    new_ty: str,
    projects_dir: Path,
    old_reports_dir: Path,
    new_reports_dir: Path,
) -> None:
    """Run memory tests for all projects with both ty versions."""
    old_reports_dir.mkdir(parents=True, exist_ok=True)
    new_reports_dir.mkdir(parents=True, exist_ok=True)

    for project_name, url in KNOWN_PROJECTS.items():
        project_path = clone_project(name=project_name, url=url, dest=projects_dir)

        # Run old ty
        old_report_path = old_reports_dir / f"{project_name}.json"
        run_ty_memory_check(
            ty_path=old_ty, project_path=project_path, output_path=old_report_path
        )

        # Run new ty
        new_report_path = new_reports_dir / f"{project_name}.json"
        run_ty_memory_check(
            ty_path=new_ty, project_path=project_path, output_path=new_report_path
        )


def cmd_compare(args: argparse.Namespace) -> None:
    """Handle the 'compare' subcommand."""
    comparisons = []

    if args.old and args.new:
        # Single file comparison
        old_report = MemoryReport.from_json(args.old)
        new_report = MemoryReport.from_json(args.new)
        project_name = args.new.stem
        comparisons.append(
            ProjectComparison(name=project_name, old=old_report, new=new_report)
        )
    elif args.old_dir and args.new_dir:
        # Directory comparison
        old_reports = load_reports_from_directory(args.old_dir)
        new_reports = load_reports_from_directory(args.new_dir)

        # Find common projects
        common_projects = set(old_reports.keys()) & set(new_reports.keys())
        if not common_projects:
            print(
                "Error: No common projects found between old and new directories",
                file=sys.stderr,
            )
            sys.exit(1)

        for project in sorted(common_projects):
            comparisons.append(
                ProjectComparison(
                    name=project,
                    old=old_reports[project],
                    new=new_reports[project],
                )
            )
    else:
        print(
            "Error: Must specify either --old/--new or --old-dir/--new-dir",
            file=sys.stderr,
        )
        sys.exit(1)

    rendered = render_summary(comparisons)

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
        print(rendered, file=sys.stderr)
    else:
        print(rendered)


def cmd_run(args: argparse.Namespace) -> None:
    """Handle the 'run' subcommand."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # Set up directories
        projects_dir = args.projects_dir or Path(tmpdir) / "ty_memory_projects"
        old_reports_dir = args.old_reports_dir or Path(tmpdir) / "ty_memory_old"
        new_reports_dir = args.new_reports_dir or Path(tmpdir) / "ty_memory_new"

        print(f"Projects directory: {projects_dir}", file=sys.stderr)
        print(f"Old reports directory: {old_reports_dir}", file=sys.stderr)
        print(f"New reports directory: {new_reports_dir}", file=sys.stderr)

        # Run memory tests
        run_memory_tests(
            old_ty=args.old_ty,
            new_ty=args.new_ty,
            projects_dir=projects_dir,
            old_reports_dir=old_reports_dir,
            new_reports_dir=new_reports_dir,
        )

        # Load and compare reports
        old_reports = load_reports_from_directory(old_reports_dir)
        new_reports = load_reports_from_directory(new_reports_dir)

        comparisons = []
        for project in sorted(old_reports.keys() & new_reports.keys()):
            comparisons.append(
                ProjectComparison(
                    name=project,
                    old=old_reports[project],
                    new=new_reports[project],
                )
            )

        rendered = render_summary(comparisons)

    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
        print(f"Output written to {args.output}", file=sys.stderr)
        print(rendered, file=sys.stderr)
    else:
        print(rendered)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    subparsers = parser.add_subparsers(dest="command", required=True)

    # Compare subcommand
    compare_parser = subparsers.add_parser(
        "compare",
        help="Compare pre-generated memory reports",
    )
    compare_parser.add_argument(
        "--old",
        type=Path,
        help="Path to old memory report JSON file",
    )
    compare_parser.add_argument(
        "--new",
        type=Path,
        help="Path to new memory report JSON file",
    )
    compare_parser.add_argument(
        "--old-dir",
        type=Path,
        help="Directory containing old memory report JSON files",
    )
    compare_parser.add_argument(
        "--new-dir",
        type=Path,
        help="Directory containing new memory report JSON files",
    )
    compare_parser.add_argument(
        "--output",
        type=Path,
        help="Write output to file instead of stdout",
    )

    # Run subcommand
    run_parser = subparsers.add_parser(
        "run",
        help="Clone projects, run ty, and compare memory usage",
    )
    run_parser.add_argument(
        "--old-ty",
        required=True,
        help="Path to old ty executable",
    )
    run_parser.add_argument(
        "--new-ty",
        required=True,
        help="Path to new ty executable",
    )
    run_parser.add_argument(
        "--projects-dir",
        type=Path,
        help="Directory to clone projects into (default: temp directory)",
    )
    run_parser.add_argument(
        "--old-reports-dir",
        type=Path,
        help="Directory for old memory reports (default: temp directory)",
    )
    run_parser.add_argument(
        "--new-reports-dir",
        type=Path,
        help="Directory for new memory reports (default: temp directory)",
    )
    run_parser.add_argument(
        "--output",
        type=Path,
        help="Write output to file instead of stdout",
    )

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    match args.command:
        case "compare":
            cmd_compare(args)
        case "run":
            cmd_run(args)
        case _:
            assert False, (
                f"Unknown subcommand {args.command!r}; is the script out of date?"
            )


if __name__ == "__main__":
    main()

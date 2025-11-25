from __future__ import annotations

import argparse
import logging
import os
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING, Final

from benchmark import Hyperfine
from benchmark.projects import ALL as ALL_PROJECTS
from benchmark.snapshot import SnapshotRunner
from benchmark.tool import Mypy, Pyrefly, Pyright, Tool, Ty
from benchmark.venv import Venv

if TYPE_CHECKING:
    from benchmark.tool import Tool

TOOL_CHOICES: Final = ["ty", "pyrefly", "mypy", "pyright"]


def main() -> None:
    """Run the benchmark."""
    parser = argparse.ArgumentParser(
        description="Benchmark ty against other packaging tools."
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Print verbose output."
    )
    parser.add_argument(
        "--warmup",
        type=int,
        help="The number of warmup runs to perform.",
        default=3,
    )
    parser.add_argument(
        "--min-runs",
        type=int,
        help="The minimum number of runs to perform.",
        default=10,
    )
    parser.add_argument(
        "--project",
        "-p",
        type=str,
        help="The project(s) to run.",
        choices=[project.name for project in ALL_PROJECTS],
        action="append",
    )
    parser.add_argument(
        "--tool",
        help="Which tool to benchmark.",
        choices=TOOL_CHOICES,
        action="append",
    )

    parser.add_argument(
        "--ty-path",
        type=Path,
        help="Path to the ty binary to benchmark.",
    )

    parser.add_argument(
        "--single-threaded",
        action="store_true",
        help="Run the type checkers single threaded",
    )

    parser.add_argument(
        "--warm",
        action=argparse.BooleanOptionalAction,
        help="Run warm benchmarks in addition to cold benchmarks (for tools supporting it)",
    )

    parser.add_argument(
        "--snapshot",
        action="store_true",
        help="Run commands and snapshot their output instead of benchmarking with hyperfine.",
    )

    parser.add_argument(
        "--accept",
        action="store_true",
        help="Accept snapshot changes (only valid with --snapshot).",
    )

    args = parser.parse_args()
    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.WARN,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    # Validate arguments.
    if args.accept and not args.snapshot:
        parser.error("--accept can only be used with --snapshot")

    if args.snapshot and args.warm:
        parser.error("--warm cannot be used with --snapshot")

    verbose = args.verbose
    warmup = args.warmup
    min_runs = args.min_runs

    # Determine the tools to benchmark, based on the user-provided arguments.
    suites: list[Tool] = []

    for tool_name in args.tool or TOOL_CHOICES:
        match tool_name:
            case "ty":
                suites.append(Ty(path=args.ty_path))
            case "pyrefly":
                suites.append(Pyrefly())
            case "pyright":
                suites.append(Pyright())
            case "mypy":
                suites.append(Mypy(warm=False))
                if args.warm:
                    suites.append(Mypy(warm=True))
            case _:
                raise ValueError(f"Unknown tool: {tool_name}")

    projects = (
        [project for project in ALL_PROJECTS if project.name in args.project]
        if args.project
        else ALL_PROJECTS
    )

    benchmark_env = os.environ.copy()

    if args.single_threaded:
        benchmark_env["TY_MAX_PARALLELISM"] = "1"

    first = True

    for project in projects:
        if skip_reason := project.skip:
            print(f"Skipping {project.name}: {skip_reason}")
            continue

        with tempfile.TemporaryDirectory() as tempdir:
            cwd = Path(tempdir)
            project.clone(cwd)

            venv = Venv.create(cwd, project.python_version)
            venv.install(project.install_arguments)

            commands = []

            for suite in suites:
                suite.write_config(project, venv)
                commands.append(suite.command(project, venv, args.single_threaded))

            if not commands:
                continue

            if not first:
                print("")
                print(
                    "-------------------------------------------------------------------------------"
                )
                print("")

            print(f"{project.name}")
            print("-" * len(project.name))
            print("")

            if args.snapshot:
                # Get the directory where run.py is located to find snapshots directory.
                script_dir = Path(__file__).parent.parent.parent
                snapshot_dir = script_dir / "snapshots"

                snapshot_runner = SnapshotRunner(
                    name=f"{project.name}",
                    commands=commands,
                    snapshot_dir=snapshot_dir,
                    accept=args.accept,
                )
                snapshot_runner.run(cwd=cwd, env=benchmark_env)
            else:
                hyperfine = Hyperfine(
                    name=f"{project.name}",
                    commands=commands,
                    warmup=warmup,
                    min_runs=min_runs,
                    verbose=verbose,
                    json=False,
                )
                hyperfine.run(cwd=cwd, env=benchmark_env)

            first = False

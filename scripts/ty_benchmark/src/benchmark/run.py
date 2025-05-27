from __future__ import annotations

import argparse
import json
import logging
import tempfile
import typing
from pathlib import Path

from benchmark import Hyperfine
from benchmark.cases import Benchmark, Mypy, Pyright, Tool, Ty, Venv
from benchmark.projects import ALL as all_projects
from benchmark.projects import DEFAULT as default_projects

if typing.TYPE_CHECKING:
    from benchmark.cases import Tool


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
        "--benchmark",
        "-b",
        type=str,
        help="The benchmark(s) to run.",
        choices=[benchmark.value for benchmark in Benchmark],
        action="append",
    )
    parser.add_argument(
        "--project",
        "-p",
        type=str,
        help="The project(s) to run.",
        choices=[project.name for project in all_projects],
        action="append",
    )
    parser.add_argument(
        "--mypy",
        help="Whether to benchmark `mypy`.",
        action="store_true",
    )
    parser.add_argument(
        "--pyright",
        help="Whether to benchmark `pyright`.",
        action="store_true",
    )
    parser.add_argument(
        "--ty",
        help="Whether to benchmark ty (assumes a ty binary exists at `./target/release/ty`).",
        action="store_true",
    )
    parser.add_argument(
        "--ty-path",
        type=Path,
        help="Path(s) to the ty binary to benchmark.",
        action="append",
    )

    args = parser.parse_args()
    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.WARN,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    verbose = args.verbose
    warmup = args.warmup
    min_runs = args.min_runs

    # Determine the tools to benchmark, based on the user-provided arguments.
    suites: list[Tool] = []
    if args.pyright:
        suites.append(Pyright())

    if args.ty:
        suites.append(Ty())

    for path in args.ty_path or []:
        suites.append(Ty(path=path))

    if args.mypy:
        suites.append(Mypy())

    # If no tools were specified, default to benchmarking all tools.
    suites = suites or [Ty(), Pyright(), Mypy()]

    # Determine the benchmarks to run, based on user input.
    benchmarks = (
        [Benchmark(benchmark) for benchmark in args.benchmark]
        if args.benchmark is not None
        else list(Benchmark)
    )

    projects = [
        project
        for project in all_projects
        if project.name in (args.project or default_projects)
    ]

    for project in projects:
        with tempfile.TemporaryDirectory() as tempdir:
            cwd = Path(tempdir)
            project.clone(cwd)

            venv = Venv.create(cwd)
            venv.install(project.dependencies)

            # Set the `venv` config for pyright. Pyright only respects the `--venvpath`
            # CLI option when `venv` is set in the configuration... 🤷‍♂️
            with open(cwd / "pyrightconfig.json", "w") as f:
                f.write(json.dumps(dict(venv=venv.name)))

            for benchmark in benchmarks:
                # Generate the benchmark command for each tool.
                commands = [
                    command
                    for suite in suites
                    if (command := suite.command(benchmark, project, venv))
                ]

                # not all tools support all benchmarks.
                if not commands:
                    continue

                print(f"{project.name} ({benchmark.value})")

                hyperfine = Hyperfine(
                    name=f"{project.name}-{benchmark.value}",
                    commands=commands,
                    warmup=warmup,
                    min_runs=min_runs,
                    verbose=verbose,
                    json=False,
                )
                hyperfine.run(cwd=cwd)

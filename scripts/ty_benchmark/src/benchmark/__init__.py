from __future__ import annotations

import logging
import os
import subprocess
import sys
from collections.abc import Mapping
from pathlib import Path
from typing import NamedTuple

if sys.platform == "win32":
    import mslex as shlex
else:
    import shlex


logger = logging.getLogger(__name__)


def benchmark_environment(
    cwd: Path, *, single_threaded: bool = False
) -> dict[str, str]:
    """Keep machine-specific user configuration out of benchmark settings."""
    config_directory = cwd / ".benchmark-user-config"
    config_directory.mkdir(exist_ok=True)
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("TY_", "PYREFLY_", "PYRIGHT_", "MYPY_", "ZUBAN_"))
        and key
        not in (
            "PYTHONPATH",
            "PYTHONHOME",
            "MYPYPATH",
            "RAYON_NUM_THREADS",
            "RUST_LOG",
            "NODE_OPTIONS",
        )
    }
    workers = "1" if single_threaded else str(os.process_cpu_count() or 1)
    return env | {
        "XDG_CONFIG_HOME": str(config_directory),
        "APPDATA": str(config_directory),
        "TY_MAX_PARALLELISM": workers,
        "RAYON_NUM_THREADS": workers,
    }


def cli_benchmark_environment(
    cwd: Path, *, single_threaded: bool = False
) -> dict[str, str]:
    """Use checker concurrency defaults independently of the LSP pool limits."""
    env = benchmark_environment(cwd, single_threaded=single_threaded)
    # Rayon still accepts its deprecated override; Python can also override CPU discovery.
    for key in ("RAYON_RS_NUM_CPUS", "PYTHON_CPU_COUNT"):
        env.pop(key, None)
    if not single_threaded:
        env.pop("TY_MAX_PARALLELISM", None)
        env.pop("RAYON_NUM_THREADS", None)
    return env


class Command(NamedTuple):
    name: str
    """The name of the command to benchmark."""

    command: list[str]
    """The command to benchmark."""

    prepare: list[str] | None = None
    """The command to run before each benchmark run."""


class Hyperfine(NamedTuple):
    name: str
    """The benchmark to run."""

    commands: list[Command]
    """The commands to benchmark."""

    warmup: int
    """The number of warmup runs to perform."""

    min_runs: int
    """The minimum number of runs to perform."""

    verbose: bool
    """Whether to print verbose output."""

    json: bool
    """Whether to export results to JSON."""

    def run(self, *, cwd: Path | None = None, env: Mapping[str, str]) -> None:
        """Run the benchmark using `hyperfine`."""
        args = [
            "hyperfine",
            # Ignore any warning/error diagnostics but fail if there are any fatal errors, incorrect configuration, etc.
            # mypy exit codes: https://github.com/python/mypy/issues/14615#issuecomment-1420163253
            # pyright exit codes: https://docs.basedpyright.com/v1.31.6/configuration/command-line/#pyright-exit-codes
            # pyrefly exit codes: Not documented
            # ty: https://docs.astral.sh/ty/reference/exit-codes/
            "--ignore-failure=1",
        ]

        # Export to JSON.
        if self.json:
            args.extend(["--export-json", f"{self.name}.json"])

        # Preamble: benchmark-wide setup.
        if self.verbose:
            args.append("--show-output")

        args.extend(["--warmup", str(self.warmup), "--min-runs", str(self.min_runs)])

        # Add all command names,
        for command in self.commands:
            args.extend(["--command-name", command.name])

        # Add all prepare statements.
        for command in self.commands:
            args.extend(["--prepare", shlex.join(command.prepare or [])])

        # Add all commands.
        for command in self.commands:
            args.append(shlex.join(command.command))

        logger.info(f"Running {args}")

        subprocess.run(args, cwd=cwd, env=env, check=True)

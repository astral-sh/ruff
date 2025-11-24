from __future__ import annotations

import logging
import shlex
import subprocess
import sys
from pathlib import Path
from typing import Mapping, NamedTuple

if sys.platform == "win32":
    import mslex as shlex
else:
    import shlex


class Command(NamedTuple):
    name: str
    """The name of the command to benchmark."""

    command: list[str]
    """The command to benchmark."""

    prepare: str | None = None
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
            args.extend(["--prepare", command.prepare or ""])

        # Add all commands.
        for command in self.commands:
            args.append(shlex.join(command.command))

        logging.info(f"Running {args}")

        subprocess.run(args, cwd=cwd, env=env)

from __future__ import annotations

import logging
import shlex
import subprocess
import typing
from pathlib import Path


class Command(typing.NamedTuple):
    name: str
    """The name of the command to benchmark."""

    command: list[str]
    """The command to benchmark."""

    prepare: str | None = None
    """The command to run before each benchmark run."""


class Hyperfine(typing.NamedTuple):
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

    def run(self, *, cwd: Path | None = None) -> None:
        """Run the benchmark using `hyperfine`."""
        args = [
            "hyperfine",
            # Most repositories have some typing errors.
            # This is annoying because it prevents us from capturing "real" errors.
            "-i",
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

        subprocess.run(
            args,
            cwd=cwd,
        )

from pathlib import Path
from ruff_ecosystem import logger
from ruff_ecosystem.models import CheckOptions, FormatOptions
import time
from asyncio import create_subprocess_exec
from subprocess import PIPE
from typing import Sequence
import re

CHECK_SUMMARY_LINE_RE = re.compile(
    r"^(Found \d+ error.*)|(.*potentially fixable with.*)$"
)


CHECK_DIFF_LINE_RE = re.compile(
    r"^(?P<pre>[+-]) (?P<inner>(?P<path>[^:]+):(?P<lnum>\d+):\d+:) (?P<post>.*)$",
)


class RuffError(Exception):
    """An error reported by ruff."""


async def ruff_check(
    *, executable: Path, path: Path, name: str, options: CheckOptions
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {executable}")
    ruff_args = ["check", "--no-cache", "--exit-zero"]
    if options.select:
        ruff_args.extend(["--select", options.select])
    if options.ignore:
        ruff_args.extend(["--ignore", options.ignore])
    if options.exclude:
        ruff_args.extend(["--exclude", options.exclude])
    if options.show_fixes:
        ruff_args.extend(["--show-fixes", "--ecosystem-ci"])

    start = time.time()
    proc = await create_subprocess_exec(
        executable.absolute(),
        *ruff_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )
    result, err = await proc.communicate()
    end = time.time()

    logger.debug(f"Finished checking {name} with {executable} in {end - start:.2f}")

    if proc.returncode != 0:
        raise RuffError(err.decode("utf8"))

    lines = [
        line
        for line in result.decode("utf8").splitlines()
        if not CHECK_SUMMARY_LINE_RE.match(line)
    ]

    return sorted(lines)


async def ruff_format(
    *, executable: Path, path: Path, name: str, options: FormatOptions
) -> Sequence[str]:
    """Run the given ruff binary against the specified path."""
    logger.debug(f"Checking {name} with {executable}")
    ruff_args = ["format", "--no-cache", "--exit-zero"]

    start = time.time()
    proc = await create_subprocess_exec(
        executable.absolute(),
        *ruff_args,
        ".",
        stdout=PIPE,
        stderr=PIPE,
        cwd=path,
    )
    result, err = await proc.communicate()
    end = time.time()

    logger.debug(f"Finished formatting {name} with {executable} in {end - start:.2f}")

    if proc.returncode != 0:
        raise RuffError(err.decode("utf8"))

    lines = result.decode("utf8").splitlines()
    return lines

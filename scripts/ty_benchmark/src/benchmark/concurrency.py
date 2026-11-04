"""CLI concurrency settings shared by every benchmark project."""

from typing import Final, Literal

type CliTool = Literal["ty", "pyrefly", "mypy", "pyright", "zuban"]

# Calibrated on the 16-core M3 Max reference host; see concurrency.md.
CLI_WORKERS: Final[dict[CliTool, int]] = {
    "ty": 16,
    "pyrefly": 16,
    "mypy": 8,
    "pyright": 8,
    "zuban": 4,
}


def cli_worker_count(tool: CliTool, *, single_threaded: bool) -> int:
    if single_threaded:
        # A positive mypy value starts external worker processes.
        return 0 if tool == "mypy" else 1
    return CLI_WORKERS[tool]

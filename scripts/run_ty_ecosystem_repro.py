#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

"""Run a reproduction command with the recorded environment and a supplied deadline."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a command after --")

    try:
        metadata = json.loads(args.metadata.read_text())
        if not isinstance(metadata, dict) or not isinstance(
            runtime := metadata.get("runtime"), dict
        ):
            raise TypeError(
                "add reviewed runtime settings to the manifest before running a reproduction"
            )
        timeout = args.timeout_seconds
        if not 0 < timeout < float("inf"):
            raise ValueError(f"invalid timeout: {timeout!r}")
        env = os.environ.copy()
        overrides = runtime["checker_env"]
        if not isinstance(overrides, dict):
            raise TypeError("invalid checker_env: expected an object")
        for name in ("TY_UV", "UV_LOCKED", "RUST_BACKTRACE"):
            if name not in overrides:
                raise ValueError(f"missing reviewed environment setting: {name}")
        for name, value in overrides.items():
            if value is None:
                env.pop(name, None)
            elif isinstance(value, str):
                env[name] = value
            else:
                raise ValueError(f"invalid environment value for {name}")

        try:
            completed = subprocess.run(
                command,
                env=env,
                timeout=timeout,
                capture_output=True,
                text=True,
                check=False,
            )
            result = {
                "timed_out": False,
                "return_code": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            }
        except subprocess.TimeoutExpired:
            # The analyzer discards partial diagnostics and stderr on a timeout.
            result = {
                "timed_out": True,
                "return_code": None,
                "stdout": "",
                "stderr": "",
            }

        args.output.write_text(json.dumps(result, indent=2) + "\n")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))

    sys.stdout.write(result["stdout"])
    sys.stderr.write(result["stderr"])
    if result["timed_out"]:
        print(f"Reproduction timed out after {timeout} seconds", file=sys.stderr)


if __name__ == "__main__":
    main()

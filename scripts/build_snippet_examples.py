"""Build rendering examples once and export their executable paths to CI tests."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile", default="dev", help="Cargo profile used by the tests"
    )
    args = parser.parse_args()
    env_file = Path(os.environ["GITHUB_ENV"])

    result = subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--package",
            "ruff_annotate_snippets",
            "--examples",
            "--all-features",
            "--profile",
            args.profile,
            "--message-format=json-render-diagnostics",
        ],
        cwd=Path(__file__).resolve().parent.parent,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )

    # Use Cargo's reported paths so custom target directories and Windows executable suffixes work.
    paths = {}
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (
            message["reason"] == "compiler-artifact"
            and message["target"]["kind"] == ["example"]
            and message["executable"] is not None
        ):
            paths[message["target"]["name"]] = message["executable"]

    if not paths:
        raise RuntimeError("Cargo did not report any rendering example executables")

    with env_file.open("a", encoding="utf-8") as file:
        for name, path in sorted(paths.items()):
            file.write(f"RUFF_ANNOTATE_SNIPPETS_EXAMPLE_{name}={path}\n")


if __name__ == "__main__":
    main()

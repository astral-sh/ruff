# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# exclude-newer = "P7D"
# ///

"""Find a reusable corpus of Python sources for a fixed seed range in GitHub Actions.

Print the artifact name and, on a hit, the ID of the run to download it from.
On a miss, the workflow uses the name to publish a fresh corpus on `main`.
Pull requests generate their inputs as they fuzz until that artifact is available.
"""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
import tomllib
from pathlib import Path
from urllib.parse import quote


def corpus_name(seeds: str) -> str:
    """Use the Python patch version, generator version, and seed range as the artifact key.

    The generator uses Python's `ast` module, which can change between patch releases.
    """
    with Path(__file__).with_name("uv.lock").open("rb") as lock:
        packages = tomllib.load(lock)["package"]
    generator = next(
        package["version"]
        for package in packages
        if package["name"] == "pysource-codegen"
    )
    return f"py-fuzzer-corpus-v1-{platform.python_version()}-{seeds}-pysource-codegen-{generator}"


def find_run(name: str) -> str | None:
    """Find an unexpired artifact published by this workflow on `main`.

    The artifacts API also returns matches from pull requests, including those
    from fork branches named `main`. Check the producing run before using a match.
    """
    repo = os.environ["GITHUB_REPOSITORY"]
    workflow = (
        os.environ["GITHUB_WORKFLOW_REF"].removeprefix(f"{repo}/").split("@", 1)[0]
    )
    api = f"repos/{repo}/actions"
    artifacts = f"{api}/artifacts?name={quote(name)}"
    unexpired_runs = ".artifacts[] | select(.expired == false) | .workflow_run.id"
    runs = subprocess.check_output(
        ["gh", "api", "--paginate", artifacts, "--jq", unexpired_runs], text=True
    )
    for run_id in runs.splitlines():
        run = json.loads(subprocess.check_output(["gh", "api", f"{api}/runs/{run_id}"]))
        if (
            run["path"] == workflow
            and run["head_branch"] == "main"
            and run["event"] in {"push", "schedule"}
        ):
            return run_id
    return None


if __name__ == "__main__":
    name = corpus_name(sys.argv[1])
    print(f"name={name}")
    if (run_id := find_run(name)) is not None:
        print(f"run-id={run_id}")

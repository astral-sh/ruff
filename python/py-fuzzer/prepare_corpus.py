"""Prepare the fixed-seed corpus used by GitHub Actions."""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor
from importlib.metadata import version
from pathlib import Path
from urllib.parse import quote

from pysource_codegen import generate

from fuzz import parse_seed_argument


def find_run(name: str) -> str | None:
    repo = os.environ["GITHUB_REPOSITORY"]
    workflow = os.environ["GITHUB_WORKFLOW_REF"].split("@", 1)[0]
    api = f"repos/{repo}/actions"
    artifacts = f"{api}/artifacts?name={quote(name)}"
    unexpired_runs = ".artifacts[] | select(.expired == false) | .workflow_run.id"
    runs = subprocess.check_output(
        ["gh", "api", "--paginate", artifacts, "--jq", unexpired_runs], text=True
    )
    # A PR can upload an artifact of the same name, even from a fork branch called `main`.
    for run_id in runs.splitlines():
        run = json.loads(subprocess.check_output(["gh", "api", f"{api}/runs/{run_id}"]))
        if (
            f"{repo}/{run['path']}" == workflow
            and run["head_branch"] == os.environ["DEFAULT_BRANCH"]
            and run["event"] in {"push", "schedule"}
        ):
            return run_id
    return None


def main() -> None:
    seed_arg = sys.argv[1]
    # The generator uses Python's `ast` module, which can change between patch releases.
    generator = version("pysource-codegen")
    name = f"py-fuzzer-corpus-v1-{platform.python_version()}-{seed_arg}-pysource-codegen-{generator}"
    print(f"name={name}")
    if (run_id := find_run(name)) is not None:
        print(f"run-id={run_id}")
        return
    if (
        os.environ["GITHUB_EVENT_NAME"] not in {"push", "schedule"}
        or os.environ["GITHUB_REF"] != f"refs/heads/{os.environ['DEFAULT_BRANCH']}"
    ):
        return

    requested = parse_seed_argument(seed_arg)
    seeds = [requested] if isinstance(requested, int) else requested
    directory = Path(os.environ["RUNNER_TEMP"]) / "fuzz-corpus"
    directory.mkdir(exist_ok=True)
    with ProcessPoolExecutor() as pool:
        for seed, source in zip(
            seeds, pool.map(generate, seeds, chunksize=16), strict=True
        ):
            (directory / f"{seed}.py").write_text(source, encoding="utf-8")
    print(f"path={directory}")


if __name__ == "__main__":
    main()

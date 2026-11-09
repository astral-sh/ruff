# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.ty.analysis]
# return-type-inference = "always"
#
# [tool.ty.rules]
# truthiness-test-of-none-union = "warn"
# blanket-ignore-comment = "warn"
# missing-type-argument = "warn"
# possibly-unresolved-reference = "warn"
# unsound-return = "warn"
# unsound-yield = "warn"
# unsupported-dynamic-base = "warn"
# division-by-zero = "warn"
# dynamic-function-decorator-return = "warn"
# unsound-assignment = "warn"
# redundant-condition-strict = "warn"
# disjoint-cast = "warn"
# missing-direct-dependency = "warn"
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
from pathlib import Path
from urllib.parse import quote


def corpus_name(seeds: str) -> str:
    """Use the Python patch version, generator version, and seed range as the artifact key.

    The generator uses Python's `ast` module, which can change between patch releases.
    """
    tree = json.loads(
        subprocess.check_output(
            [
                "uv",
                "tree",
                "--quiet",
                "--frozen",
                "--package=pysource-codegen",
                "--depth=0",
                "--format=json",
            ],
            cwd=Path(__file__).parent,
        )
    )
    root = tree["roots"][0]["id"]
    version = tree["resolution"][root]["version"]
    return f"py-fuzzer-corpus-v1-{platform.python_version()}-{seeds}-pysource-codegen-{version}"


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
    pages = json.loads(
        subprocess.check_output(["gh", "api", "--paginate", "--slurp", artifacts])
    )
    for page in pages:
        for artifact in page["artifacts"]:
            if artifact["expired"]:
                continue
            run_id = str(artifact["workflow_run"]["id"])
            run = json.loads(
                subprocess.check_output(["gh", "api", f"{api}/runs/{run_id}"])
            )
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

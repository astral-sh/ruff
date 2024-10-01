"""This is @konstin's scripts for checking an entire checkout of ~2.1k packages for
panics, fix errors and similar problems.

It's a less elaborate, more hacky version of check_ecosystem.py
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from subprocess import CalledProcessError
from typing import NamedTuple

from tqdm import tqdm


class Repository(NamedTuple):
    """A GitHub repository at a specific ref."""

    org: str
    repo: str
    ref: str | None


def main() -> None:
    ruff_args = sys.argv[1:]
    checkouts = Path("checkouts")
    out_dir = Path("ecosystem_all_results")
    github_search_json = Path("github_search.jsonl")
    # Somehow it doesn't like plain ruff
    ruff = Path.cwd().joinpath("ruff")

    out_dir.mkdir(parents=True, exist_ok=True)

    repositories = []
    for line in github_search_json.read_text().splitlines():
        item = json.loads(line)
        # Pick only the easier case for now.
        if item["path"] != "pyproject.toml":
            continue
        repositories.append(
            Repository(
                item["owner"],
                item["repo"],
                item.get("ref"),
            ),
        )

    successes = 0
    errors = 0
    for repository in tqdm(repositories):
        project_dir = checkouts.joinpath(f"{repository.org}:{repository.repo}")
        if not project_dir.is_dir():
            tqdm.write(f"Missing {project_dir}")
            errors += 1
            continue

        try:
            output = subprocess.run(
                [ruff, *ruff_args, "."],
                cwd=project_dir,
                capture_output=True,
                text=True,
            )
        except CalledProcessError as e:
            tqdm.write(f"Ruff failed on {project_dir}: {e}")
            errors += 1
            continue

        org_repo = f"{repository.org}:{repository.repo}"
        out_dir.joinpath(f"{org_repo}.stdout.txt").write_text(output.stdout)
        out_dir.joinpath(f"{org_repo}.stderr.txt").write_text(output.stderr)
        successes += 1
    print(f"Success: {successes} Error {errors}")


if __name__ == "__main__":
    main()

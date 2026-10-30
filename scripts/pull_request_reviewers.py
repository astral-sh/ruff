# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///
"""Request a reviewer from repository pools using shared round-robin credits."""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

REPOSITORY = "astral-sh/ruff"
REPOSITORY_ID = 523043277
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
State = dict[str, dict[str, float]]


@dataclass(frozen=True)
class Pool:
    name: str
    patterns: tuple[str, ...]
    reviewers: tuple[str, ...]


def api(endpoint: str, *, data: dict | None = None) -> Any:
    request = Request(
        f"{API_ROOT}/{endpoint}",
        data=json.dumps(data).encode() if data is not None else None,
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        method="POST" if data is not None else "GET",
    )
    with urlopen(request, timeout=30) as response:
        return json.load(response)


def changed_files(pull_request: int) -> list[str]:
    files = []
    page = 1
    while True:
        batch = api(f"pulls/{pull_request}/files?per_page=100&page={page}")
        files.extend(item["filename"] for item in batch)
        if len(batch) < 100:
            return files
        page += 1


def parse_pools(contents: str) -> list[Pool]:
    return [
        Pool(
            name=pool["name"],
            patterns=tuple(path.lstrip("/") for path in pool["paths"]),
            reviewers=tuple(pool["reviewers"]),
        )
        for pool in tomllib.loads(contents)["pools"]
    ]


def matching_pools(pools: list[Pool], files: list[str]) -> list[tuple[Pool, int]]:
    matched = []
    for pool in pools:
        weight = sum(
            any(
                PurePosixPath(filename).full_match(pattern) for pattern in pool.patterns
            )
            for filename in files
        )
        if weight:
            matched.append((pool, weight))
    return matched


def parse_state(contents: str) -> State:
    parsed = json.loads(contents)
    if not isinstance(parsed, dict):
        raise TypeError("Reviewer state must be an object.")
    state: State = {}
    for name, value in parsed.items():
        if not isinstance(value, dict):
            raise TypeError(f"Reviewer state for pool {name} must be an object.")
        credits = {}
        for login, credit in value.items():
            if type(credit) not in (int, float) or not math.isfinite(credit):
                raise ValueError(f"Invalid reviewer credit in pool {name}.")
            credits[login] = float(credit)
        state[name] = credits
    return state


def load_state(path: Path) -> State:
    if path.exists():
        return parse_state(path.read_text())
    return {}


def eligible_pull(pull: dict, pull_request: int, head_sha: str) -> bool:
    return (
        pull["number"] == pull_request
        and pull["state"] == "open"
        and not pull["draft"]
        and pull["base"]["repo"]["id"] == REPOSITORY_ID
        and (pull["head"]["repo"] or {}).get("id") == REPOSITORY_ID
        and pull["head"]["sha"] == head_sha
    )


def has_reviewer(pull: dict) -> bool:
    author = pull["user"]["login"].lower()
    return any(user["login"].lower() != author for user in pull["requested_reviewers"])


def assign_reviewer(
    pull_request: int,
    head_sha: str,
    pools: list[Pool],
    state: State,
    *,
    dry_run: bool = False,
    rng: random.Random | None = None,
) -> tuple[str, str] | None:
    rng = rng or random.Random()
    pull = api(f"pulls/{pull_request}")
    if not eligible_pull(pull, pull_request, head_sha) or has_reviewer(pull):
        return None
    author = pull["user"]["login"].lower()
    matched = matching_pools(pools, changed_files(pull_request))
    while matched:
        eligible_pools, weights = zip(*matched, strict=True)
        pool = rng.choices(eligible_pools, weights=weights)[0]
        stored = state.get(pool.name, {})
        credits = {
            login: min(1.0, stored.get(login, 0.0) + 1 / len(pool.reviewers))
            for login in pool.reviewers
        }
        candidates = [login for login in pool.reviewers if login.lower() != author]
        while candidates:
            highest = max(credits[login] for login in candidates)
            reviewer = rng.choice(
                [login for login in candidates if credits[login] == highest]
            )
            candidates.remove(reviewer)
            current = api(f"pulls/{pull_request}")
            if not eligible_pull(current, pull_request, head_sha) or has_reviewer(
                current
            ):
                return None
            if dry_run:
                print(f"Would request review from {reviewer} in pool {pool.name}.")
                return pool.name, reviewer
            try:
                api(
                    f"pulls/{pull_request}/requested_reviewers",
                    data={"reviewers": [reviewer]},
                )
            except HTTPError as error:
                error.close()
                if error.code == 422:
                    print(f"GitHub rejected reviewer {reviewer}.", file=sys.stderr)
                    continue
                raise
            credits[reviewer] = max(-1.0, credits[reviewer] - 1)
            state[pool.name] = credits
            print(f"Requested review from {reviewer} in pool {pool.name}.")
            return pool.name, reviewer
        matched = [entry for entry in matched if entry[0] != pool]
    return None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--state", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if os.environ.get("GITHUB_REPOSITORY", REPOSITORY) != REPOSITORY:
        parser.error(f"This script only supports {REPOSITORY}.")
    if args.pull_request <= 0:
        parser.error("The pull request number must be positive.")
    if not args.dry_run and args.output is None:
        parser.error("--output is required when requesting a review.")
    pools = parse_pools(Path(".github/pr-reviewer-pools.toml").read_text())
    state = load_state(args.state)
    assign_reviewer(
        args.pull_request, args.head_sha, pools, state, dry_run=args.dry_run
    )
    if not args.dry_run:
        # Rewriting unchanged state also renews its retention after a no-op request.
        args.output.write_text(json.dumps(state, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()

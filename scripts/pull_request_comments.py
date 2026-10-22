# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Look up automation report comments by cached ID, with discovery on a cache miss."""

from __future__ import annotations

import argparse
import json
import os
import re
from typing import Any, NamedTuple
from urllib.error import HTTPError
from urllib.request import Request, urlopen

REPOSITORY = "astral-sh/ruff"
REPOSITORY_ID = 523043277
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
BOT_ID = 305554984  # astral-automations-bot


class Report(NamedTuple):
    marker: str
    workflow: str


REPORTS = {
    "ruff-ecosystem": Report("<!-- generated-comment ecosystem -->", "ci.yaml"),
    "memory": Report(
        "<!-- generated-comment memory_report_diff -->", "memory_report.yaml"
    ),
    "typing-conformance": Report(
        "<!-- generated-comment typing_conformance_diagnostics_diff -->",
        "typing_conformance.yaml",
    ),
    "ty-ecosystem": Report(
        "<!-- generated-comment ty ecosystem-analyzer -->",
        "ty-ecosystem-analyzer.yaml",
    ),
}


def api(
    endpoint: str,
    *,
    method: str = "GET",
    body: str | None = None,
    missing_ok: bool = False,
) -> Any:
    request = Request(
        f"{API_ROOT}/{endpoint}",
        data=json.dumps({"body": body}).encode() if body is not None else None,
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        method=method,
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        if missing_ok and error.code == 404:
            return None
        raise


def pages(endpoint: str):
    page = 1
    separator = "&" if "?" in endpoint else "?"
    while True:
        items = api(f"{endpoint}{separator}per_page=100&page={page}")
        yield from items
        if len(items) < 100:
            return
        page += 1


def matches_comment(comment: dict, pull_request: int, marker: str) -> bool:
    author = comment.get("user") or {}
    return (
        author.get("id") == BOT_ID
        and author.get("type") == "Bot"
        and comment.get("issue_url") == f"{API_ROOT}/issues/{pull_request}"
        and (comment.get("body") or "").startswith(f"{marker}\n")
    )


def parse_comment_id(value: str) -> int | None:
    # Cache values are untrusted, including values from other jobs on the same PR.
    if re.fullmatch(r"[1-9][0-9]{0,19}", value.strip()):
        return int(value)
    return None


def find_report(
    pull_request: int, report: str, comment_id: int | None = None
) -> dict | None:
    if comment_id is not None:
        comment = api(f"issues/comments/{comment_id}", missing_ok=True)
        if comment and matches_comment(comment, pull_request, REPORTS[report].marker):
            return comment
    # Discovery repairs expired or invalid cache entries and recovers a comment
    # created before a cache write failed.
    return next(
        (
            comment
            for comment in pages(f"issues/{pull_request}/comments")
            if matches_comment(comment, pull_request, REPORTS[report].marker)
        ),
        None,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--report", required=True, choices=REPORTS)
    parser.add_argument("--comment-id", default="")
    args = parser.parse_args()
    if os.environ.get("GITHUB_REPOSITORY", REPOSITORY) != REPOSITORY:
        parser.error(f"This script only supports {REPOSITORY}.")
    if args.pull_request <= 0:
        parser.error("The pull request number must be positive.")
    if comment := find_report(
        args.pull_request, args.report, parse_comment_id(args.comment_id)
    ):
        print(comment["id"])


if __name__ == "__main__":
    main()

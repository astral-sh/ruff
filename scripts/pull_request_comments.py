# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Look up automation report comments using IDs from the key-value store."""

from __future__ import annotations

import argparse
import json
import os
import re
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

REPOSITORY = "astral-sh/ruff"
REPOSITORY_ID = 523043277
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
BOT_ID = 305554984  # astral-automations-bot


@dataclass(frozen=True)
class Report:
    workflow: str
    artifact: str
    filename: str


REPORTS = {
    "ruff-ecosystem": Report(
        workflow="ci.yaml",
        artifact="ecosystem-result",
        filename="ecosystem-result",
    ),
    "memory": Report(
        workflow="memory_report.yaml",
        artifact="memory_report_diff",
        filename="memory_report_diff.diff",
    ),
    "typing-conformance": Report(
        workflow="typing_conformance.yaml",
        artifact="typing_conformance_diagnostics_diff",
        filename="typing_conformance_diagnostics.diff",
    ),
    "ty-ecosystem": Report(
        workflow="ty-ecosystem-analyzer.yaml",
        artifact="comment.md",
        filename="comment.md",
    ),
}


def report_marker(report: str) -> str:
    return f"<!-- generated-comment {report} -->"


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


def matches_comment(comment: dict, pull_request: int, marker: str) -> bool:
    author = comment.get("user") or {}
    return (
        author.get("id") == BOT_ID
        and author.get("type") == "Bot"
        and comment.get("issue_url") == f"{API_ROOT}/issues/{pull_request}"
        and (comment.get("body") or "").startswith(f"{marker}\n")
    )


def parse_comment_id(value: str) -> int:
    # Stored values are untrusted, including values from other jobs on the same PR.
    if not re.fullmatch(r"[1-9][0-9]{0,19}", value):
        raise ValueError("The stored comment ID must be a positive decimal integer.")
    return int(value)


def find_report(
    pull_request: int, report: str, comment_id: int | None = None
) -> dict | None:
    if comment_id is None:
        return None
    comment = api(f"issues/comments/{comment_id}", missing_ok=True)
    if comment is not None and not matches_comment(
        comment, pull_request, report_marker(report)
    ):
        raise ValueError(
            "The stored ID does not identify this bot's report on this PR."
        )
    return comment


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--report", required=True, choices=REPORTS)
    parser.add_argument("--comment-id", type=parse_comment_id)
    args = parser.parse_args()
    if os.environ.get("GITHUB_REPOSITORY", REPOSITORY) != REPOSITORY:
        parser.error(f"This script only supports {REPOSITORY}.")
    if args.pull_request <= 0:
        parser.error("The pull request number must be positive.")
    if comment := find_report(args.pull_request, args.report, args.comment_id):
        print(comment["id"])


if __name__ == "__main__":
    main()
